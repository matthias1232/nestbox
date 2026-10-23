"""Containers and services move from ``apps.core`` into their own module.

The rows already live in ``core_container`` and ``core_service``; both the models and the
migrations record those table names, so this migration only teaches Django about them. On a
fresh install ``core.0001`` still creates the tables, and everything from here on is state
bookkeeping.
"""
from django.db import migrations, models
import django.db.models.deletion


class Migration(migrations.Migration):

    initial = True

    dependencies = [
        ("core", "0001_initial"),
    ]

    operations = [
        migrations.SeparateDatabaseAndState(
            state_operations=[
                migrations.CreateModel(
                    name="Container",
                    fields=[
                        ("id", models.BigAutoField(auto_created=True, primary_key=True, serialize=False, verbose_name="ID")),
                        ("name", models.CharField(max_length=200)),
                        ("container_id", models.CharField(blank=True, db_index=True, max_length=64)),
                        ("image", models.CharField(blank=True, max_length=400)),
                        ("image_digest", models.CharField(blank=True, max_length=100)),
                        ("role", models.CharField(choices=[("controller", "controller"), ("gateway", "gateway"), ("traffic", "traffic"), ("dns", "dns"), ("nest", "nest"), ("other", "other")], default="other", max_length=20)),
                        ("state", models.CharField(choices=[("created", "created"), ("running", "running"), ("paused", "paused"), ("restarting", "restarting"), ("exited", "exited"), ("dead", "dead"), ("unknown", "unknown")], default="unknown", max_length=20)),
                        ("health", models.CharField(blank=True, max_length=30)),
                        ("status_text", models.CharField(blank=True, max_length=200)),
                        ("ports", models.JSONField(blank=True, default=list)),
                        ("labels", models.JSONField(blank=True, default=dict)),
                        ("started_at", models.DateTimeField(blank=True, null=True)),
                        ("updated_at", models.DateTimeField(auto_now=True)),
                        ("node", models.ForeignKey(on_delete=django.db.models.deletion.CASCADE, related_name="containers", to="core.node")),
                    ],
                    options={
                        "ordering": ["role", "name"],
                        "db_table": "core_container",
                    },
                ),
                migrations.CreateModel(
                    name="Service",
                    fields=[
                        ("id", models.BigAutoField(auto_created=True, primary_key=True, serialize=False, verbose_name="ID")),
                        ("name", models.CharField(default="ui", max_length=100)),
                        ("port", models.PositiveIntegerField()),
                        ("url", models.URLField(blank=True, max_length=500)),
                        ("tls_domain", models.CharField(blank=True, max_length=255)),
                        ("is_public", models.BooleanField(default=False)),
                        ("created_at", models.DateTimeField(auto_now_add=True)),
                        ("container", models.ForeignKey(on_delete=django.db.models.deletion.CASCADE, related_name="services", to="containers.container")),
                    ],
                    options={
                        "ordering": ["container", "port"],
                        "db_table": "core_service",
                    },
                ),
                migrations.AddIndex(
                    model_name="container",
                    index=models.Index(fields=["state"], name="core_contai_state_0164e8_idx"),
                ),
                migrations.AddConstraint(
                    model_name="container",
                    constraint=models.UniqueConstraint(fields=("node", "name"), name="uniq_container_per_node"),
                ),
                migrations.AddConstraint(
                    model_name="service",
                    constraint=models.UniqueConstraint(fields=("container", "name"), name="uniq_service_per_container"),
                ),
            ],
        ),
    ]
