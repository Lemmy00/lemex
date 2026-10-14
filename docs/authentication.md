# API setup

Lemex connects through an API key. It does not offer OpenAI or ChatGPT login.

```sh
lemex setup
```

Accept the default RCP URL (`https://inference.rcp.epfl.ch/v1`) or enter another Responses API endpoint, then paste your API key at the hidden prompt. Setup saves the key in the configured credential store (`~/.lemex/auth.json` by default, with user-only permissions on Unix). Set `cli_auth_credentials_store = "keyring"` or `"auto"` in `config.toml` to use the system credential store.

To replace the key, pipe it to setup without putting it in command arguments:

```sh
printenv LEMEX_API_KEY | lemex setup --with-api-key
```

To update the endpoint or model while retaining the saved key:

```sh
lemex setup --base-url "https://api.example.com/v1" --model "your-model-id"
```

The endpoint must support the Responses API. Choose a model supported by that endpoint and adapt the model catalog if needed; see [Configuration](config.md).

For environment-only use, set `LEMEX_API_KEY`. It takes precedence over the saved key. `LEMEX_BASE_URL` overrides the saved endpoint while set. Custom providers use their configured `env_key`.

Run `lemex doctor` to check configuration and credentials. To verify a model request:

```sh
lemex exec --ephemeral --skip-git-repo-check "Reply exactly OK"
```
