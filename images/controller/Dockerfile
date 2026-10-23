FROM python:3.12-slim AS build

ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends build-essential libpq-dev \
    && rm -rf /var/lib/apt/lists/*
COPY pyproject.toml ./
COPY controlplane ./controlplane
RUN pip install --no-cache-dir .

FROM python:3.12-slim

ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1 DJANGO_SETTINGS_MODULE=config.settings.prod
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends libpq5 tini gettext \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home nestbox
COPY --from=build /usr/local/lib/python3.12/site-packages /usr/local/lib/python3.12/site-packages
COPY --from=build /usr/local/bin /usr/local/bin
COPY --chown=nestbox:nestbox controlplane ./controlplane
COPY --chown=nestbox:nestbox packs ./packs
COPY --chown=nestbox:nestbox docs ./docs
COPY --chown=nestbox:nestbox images/controller/entrypoint.sh /app/images/controller/entrypoint.sh
USER nestbox
WORKDIR /app/controlplane
# collectstatic only needs the settings to import; the runtime secret is injected by compose.
RUN NESTBOX_SECRET_KEY=collectstatic-only STATIC_ROOT=/app/staticfiles \
    python manage.py collectstatic --noinput

EXPOSE 8000
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/images/controller/entrypoint.sh"]
