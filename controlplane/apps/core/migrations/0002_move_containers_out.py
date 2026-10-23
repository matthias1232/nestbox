"""Core hands the container models over to ``apps.containers``.

State only: the tables stay where they are, and ``containers.0001`` now owns their definition.
"""
from django.db import migrations


class Migration(migrations.Migration):

    dependencies = [
        ("core", "0001_initial"),
        ("containers", "0001_initial"),
    ]

    operations = [
        migrations.SeparateDatabaseAndState(
            state_operations=[
                migrations.DeleteModel(name="Service"),
                migrations.DeleteModel(name="Container"),
            ],
        ),
    ]
