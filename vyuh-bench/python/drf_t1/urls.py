from django.urls import path

from . import views

urlpatterns = [
    path("health", views.health),
    path("echo", views.echo),
    path("items/<int:item_id>", views.item),
]

