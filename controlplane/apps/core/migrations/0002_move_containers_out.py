"""Core hands the container models over to ``apps.containers``.

State only: the tables stay where they are, and ``containers.0001`` now owns their definition.

There is deliberately no dependency on ``containers.0001``: containers is a module and may not be
installed at all. This migration only removes two models from *core's* state, which is complete
after ``core.0001``; a core-only deployment must be able to migrate without the containers app.
The database operations are empty, so the existing tables and their rows are untouched.
"""
from django.db import migrations


class Migration(migrations.Migration):

    dependencies = [
        ("core", "0001_initial"),
    ]

    operations = [
        migrations.SeparateDatabaseAndState(
            state_operations=[
                migrations.DeleteModel(name="Service"),
                migrations.DeleteModel(name="Container"),
            ],
        ),
    ]
