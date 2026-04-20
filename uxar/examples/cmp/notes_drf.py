"""
Getting Started — Notes API  (Django REST Framework equivalent)

Equivalent to uxar/examples/notes.rs — same endpoints, same JWT cookie auth,
same role model, same cron job, same auto-generated OpenAPI docs.

Install:
    pip install django djangorestframework djangorestframework-simplejwt \
                django-apscheduler drf-spectacular psycopg2-binary

settings.py additions needed:
    INSTALLED_APPS    += ["rest_framework", "drf_spectacular", "django_apscheduler"]
    DATABASES["default"] = env.db("DATABASE_URL")
    REST_FRAMEWORK = {
        "DEFAULT_AUTHENTICATION_CLASSES": ["rest_framework_simplejwt.authentication.JWTAuthentication"],
        "DEFAULT_PERMISSION_CLASSES":     ["rest_framework.permissions.IsAuthenticated"],
        "DEFAULT_SCHEMA_CLASS":           "drf_spectacular.openapi.AutoSchema",
    }

Run:
    DATABASE_URL=postgres://user:pass@localhost/notes_db \
    SECRET_KEY=change-me-in-production \
    python manage.py runserver 8080

OpenAPI docs: http://localhost:8080/api/docs/
"""

# ── urls.py ───────────────────────────────────────────────────────────────────
#
# from django.urls import path
# from drf_spectacular.views import SpectacularAPIView, SpectacularSwaggerView
# urlpatterns = [
#     path("v1/login",       views.login),
#     path("v1/notes",       views.NoteListCreate.as_view()),
#     path("v1/notes/all",   views.PurgeNotes.as_view()),
#     path("api/schema",     SpectacularAPIView.as_view()),
#     path("api/docs/",      SpectacularSwaggerView.as_view(url_name="schema")),
# ]

import logging

from apscheduler.schedulers.background import BackgroundScheduler
from django.apps import AppConfig
from django.db import models, connection
from rest_framework import serializers, status
from rest_framework.decorators import api_view, permission_classes
from rest_framework.permissions import AllowAny, BasePermission, IsAuthenticated
from rest_framework.request import Request
from rest_framework.response import Response
from rest_framework.views import APIView
from rest_framework_simplejwt.tokens import RefreshToken

# ── Roles ─────────────────────────────────────────────────────────────────────

class Role:
    User  = 1 << 0
    Admin = 1 << 1

class HasRole(BasePermission):
    def __init__(self, role: int):
        self.role = role
    def has_permission(self, request: Request, view) -> bool:
        roles = getattr(request.user, "role_mask", 0)
        return bool(roles & self.role)

def require(role: int):
    return [IsAuthenticated, HasRole(role)]

# ── Models ────────────────────────────────────────────────────────────────────

class Note(models.Model):
    owner = models.CharField(max_length=255)
    title = models.CharField(max_length=255)
    body  = models.TextField()

    class Meta:
        db_table = "notes"

class NoteSerializer(serializers.ModelSerializer):
    class Meta:
        model  = Note
        fields = ["id", "owner", "title", "body"]

class NoteInputSerializer(serializers.Serializer):
    title = serializers.CharField()
    body  = serializers.CharField()

class LoginSerializer(serializers.Serializer):
    username = serializers.CharField()
    password = serializers.CharField()

# ── Handlers ──────────────────────────────────────────────────────────────────

@api_view(["POST"])
@permission_classes([AllowAny])
def login(request: Request) -> Response:
    """Authenticate; sets JWT access + refresh cookies on success."""
    s = LoginSerializer(data=request.data)
    s.is_valid(raise_exception=True)
    # TODO: verify against your users table with a hashed password check.
    if s.validated_data["username"] != "alice" or s.validated_data["password"] != "secret":
        return Response({"detail": "Invalid credentials"}, status=status.HTTP_401_UNAUTHORIZED)
    user = request.user.__class__(username=s.validated_data["username"])
    user.role_mask = Role.User
    refresh = RefreshToken.for_user(user)
    resp = Response()
    resp.set_cookie("access_token",  str(refresh.access_token), httponly=True, samesite="Lax")
    resp.set_cookie("refresh_token", str(refresh),              httponly=True, samesite="Strict")
    return resp


class NoteListCreate(APIView):
    permission_classes = require(Role.User)

    def get(self, request: Request) -> Response:
        """List all notes belonging to the authenticated user."""
        notes = Note.objects.filter(owner=request.user.username)
        return Response(NoteSerializer(notes, many=True).data)

    def post(self, request: Request) -> Response:
        """Create a note; returns the saved note with its id."""
        s = NoteInputSerializer(data=request.data)
        s.is_valid(raise_exception=True)
        note = Note.objects.create(owner=request.user.username, **s.validated_data)
        return Response(NoteSerializer(note).data, status=status.HTTP_201_CREATED)


class PurgeNotes(APIView):
    permission_classes = require(Role.Admin)

    def delete(self, request: Request) -> Response:
        """Delete all notes. Requires Admin role."""
        deleted, _ = Note.objects.all().delete()
        return Response({"deleted": deleted})

# ── Cron ──────────────────────────────────────────────────────────────────────

def nightly_prune():
    """Fire every night at midnight (extend this to run cleanup queries)."""
    logging.info("nightly prune fired", extra={"triggered_by": "cron"})


class NotesConfig(AppConfig):
    name = "notes"

    def ready(self):
        scheduler = BackgroundScheduler()
        scheduler.add_job(nightly_prune, "cron", hour=0, minute=0, second=0)
        scheduler.start()
