# models/ — the model catalog and hardware fit

## Purpose

Answer one question before anything is clicked: **what can this machine run?**

## Rules

- The fit verdict and its reason come from the server. This screen renders them;
  it never recomputes them. If it did, the badge and the install gate could
  disagree, and the user would see "can install" on a model the API refuses.
- A cloud model is labelled as such. It is not installed here, and confusing the
  two makes the GPU story a lie.
- Install and uninstall are jobs. The screen shows the job, not a spinner that
  claims success.
