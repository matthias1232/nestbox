# jobs/ — jobs and schedules

## Purpose

The operational record: what ran, what is running, what failed and why.

## Why this is a screen and not a toast

A bulk action over fifty sandboxes can half-succeed. A toast that says "done"
loses the half that failed. A job keeps the per-target result, so the retry is
informed rather than blind.
