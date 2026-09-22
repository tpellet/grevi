# Preview

Ruff includes an opt-in preview mode to provide an opportunity for community feedback and increase confidence that
changes are a net-benefit before enabling them for everyone.

Preview mode enables a collection of unstable features such as new lint rules and fixes, formatter style changes, interface updates, and more. Warnings about deprecated features may turn into errors when using preview mode.

Enabling preview mode does not on its own enable all preview rules. See the [rules section](#using-rules-that-are-in-preview) for details on selecting preview rules.

## Enabling preview mode

Preview mode can be enabled with the `--preview` flag on the CLI or by setting `preview = true` in your Ruff
configuration file.

Preview mode can be configured separately for linting and formatting. To enable preview lint rules without preview style formatting:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    preview = true
    ```

=== "ruff.toml"

    ```toml
    [lint]
    preview = true
    ```

=== "CLI"

    ```console
    ruff check --preview
    ```


To enable preview style formatting without enabling any preview lint rules:

=== "pyproject.toml"

    ```toml
    [tool.ruff.format]
    preview = true
    ```

=== "ruff.toml"

    ```toml
    [format]
    preview = true
    ```

=== "CLI"

    ```console
    ruff format --preview
    ```

## Using rules that are in preview
