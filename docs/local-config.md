# Local Config

ProCMP separates **shared pipeline config** from **personal preferences**

## Fields

| Field | Description |
|-------|-------------|
| `openComposedOutput` | Command to open the composed output after each build |
| `releaseNotesEditor` | Command to edit release notes before deploy |
| `envFile` | Path to dotenv file (default: `.env`) |

---

## Example

=== "VS Code user"

    ```json title="pipeline/.pcmp.local.json"
    {
        "openComposedOutput": {
            "command": "code",
            "args": ["{file}"]
        },
        "releaseNotesEditor": {
            "command": "code",
            "args": ["--wait", "{file}"]
        }
    }
    ```

=== "Neovim user"

    ```json title="pipeline/.pcmp.local.json"
    {
        "openComposedOutput": {
            "command": "nvim",
            "args": ["{file}"]
        },
        "releaseNotesEditor": {
            "command": "nvim",
            "args": ["{file}"]
        }
    }
    ```

=== "Cursor user"

    ```json title="pipeline/.pcmp.local.json"
    {
        "openComposedOutput": {
            "command": "cursor",
            "args": ["{file}"]
        },
        "releaseNotesEditor": {
            "command": "cursor",
            "args": ["--wait", "{file}"]
        }
    }
    ```

---

## Setup

1. Create `pipeline/.pcmp.local.json` (or wherever your `.pcmp.json` lives)
2. Add your preferred fields (see table above)
3. The file is gitignored automatically by `pcmp init`, or add it manually:

    ```gitignore title=".gitignore"
    pipeline/.pcmp.local.json
    ```

---

## Sharing a template

Commit a `.pcmp.local.example.json` alongside your `.pcmp.json` so teammates know what fields are available:

```json title="pipeline/.pcmp.local.example.json"
{
    "openComposedOutput": {
        "command": "code",
        "args": ["{file}"]
    },
    "releaseNotesEditor": {
        "command": "code",
        "args": ["--wait", "{file}"]
    }
}
```

The `{file}` placeholder is replaced with the actual file path at runtime.
