# ProCMP

**Build composition and release pipeline for Luau projects.**

ProCMP is a powerful CLI tool that bundles your source with [darklua](https://github.com/seaofvoices/darklua) using build configurations, injects it into a frame with macros, and deploys it to Github releases. All in one mighty package.

---

## Quick Start

```sh
# Install via Aftman
aftman add seaofvoices/darklua
aftman add Proton-Utilities/ProCMP

# Scaffold a new project
pcmp init

# Run the pipeline
pcmp pipeline/.pcmp.json
```

---

!!! tip "New to ProCMP?"
    Start with [Getting Started](getting-started.md) for a step-by-step walkthrough.
