from django.urls import path

from . import views

urlpatterns = [
    path("health", views.health),
    path("projects", views.create_project),
    path("projects/<int:project_id>/events", views.project_events),
    path("projects/<int:project_id>/summary", views.summary),
    path("projects/<int:project_id>/stream", views.stream),
    path("projects/<int:project_id>/poll", views.poll),
]
