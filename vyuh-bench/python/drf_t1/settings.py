import os

DB = os.environ.get("T1_SQLITE_PATH", "/tmp/vyuh_bench_t1_drf.sqlite3")

DEBUG = False
SECRET_KEY = "bench"
ROOT_URLCONF = "drf_t1.urls"
ALLOWED_HOSTS = ["*"]
INSTALLED_APPS = ["rest_framework"]
MIDDLEWARE = []
DATABASES = {"default": {"ENGINE": "django.db.backends.sqlite3", "NAME": DB}}
REST_FRAMEWORK = {"UNAUTHENTICATED_USER": None}

