# fastverk/desktop

This repository is a **git / CI / release vehicle** for the fastverk desktop/runtime Bazel modules (`fvkit`, `fastverk-app`). It is **not** a Bazel module.

**Git repo ≠ Bazel module.** Each subdirectory keeps its own `MODULE.bazel`, version, and tests.

Consumers keep writing:

```python
bazel_dep(name = "fvkit", version = "0.0.8")
bazel_dep(name = "fastverk-app", version = "0.0.2")
```

See `LEDGER.md` (added in the first import PR) for provenance.
