# API setup

Lemex connects through an API key. It does not offer OpenAI or ChatGPT login.

```sh
lemex setup
```

Setup shows the RCP endpoint (`https://inference.rcp.epfl.ch/v1`) and asks for your API key at a hidden prompt. Press Enter to keep an existing key. It then shows your saved models and their availability, with a search option for adding others, and lets you choose reasoning. Setup saves the key in the configured credential store (`~/.lemex/auth.json` by default, with user-only permissions on Unix). Set `cli_auth_credentials_store = "keyring"` or `"auto"` in `config.toml` to use the system credential store.

To use a different Responses API endpoint:

```sh
lemex setup --base-url "https://your-server.example/v1"
```

To replace the key, pipe it to setup without putting it in command arguments:

```sh
printenv LEMEX_API_KEY | lemex setup --with-api-key
```

To update the endpoint or model while retaining the saved key:

```sh
lemex setup --base-url "https://api.example.com/v1" --model "your-model-id"
```

The endpoint must support the Responses API. Use `lemex models` to see its current model IDs and reasoning settings; see [Model selection](#model-selection).

For environment-only use, set `LEMEX_API_KEY`. It takes precedence over the saved key. `LEMEX_BASE_URL` overrides the saved endpoint while set. Custom providers use their configured `env_key`.

Run `lemex doctor` to check configuration and credentials. To verify a model request:

```sh
lemex exec --ephemeral --skip-git-repo-check "Reply exactly OK"
```

## Model selection

```sh
lemex models                         # Your saved models and availability.
lemex models --all                   # All text models listed for your key.
lemex models --search qwen            # Search available names and IDs.
lemex models add MODEL_ID             # Add to your saved list without switching.
lemex models remove MODEL_ID          # Remove from the saved list.
lemex models reset                    # Restore the curated defaults.
lemex model                          # Choose and save a model interactively.
lemex model deepseek-ai/DeepSeek-V4.1-Flash-1017 --reasoning max
lemex model --reasoning high          # Change the saved model's reasoning.
lemex model Qwen/Qwen3.5-397B-A17B     # Switch and clear the previous override.
lemex model --reasoning default       # Use the selected model's default.
lemex models --json                   # Machine-readable IDs and capabilities.
```

Inside Lemex, `/model` shows the six curated models by default: the two DeepSeek V4.1 Flash entries, the two Qwen 3.8 entries, GLM 5.3 Flash, and Leanstral. Models not listed for the current key are shown as unavailable and cannot be selected. Choose **Add model…**, type part of a name or ID, then select a model and its reasoning. The selection is added to your saved list for future sessions. Esc returns to your saved models; canceling the reasoning choice does not add a model.

The search uses the full live catalog available to your key, excluding embedding, reranking, and transcription deployments. A provider refresh preserves the search text. `lemex model` offers the same saved-list and search flow in the terminal. Selecting an available model directly with `lemex model MODEL_ID` also adds it to your list.

Reasoning settings are model-specific. Deployment-advertised effort settings take precedence; otherwise Lemex uses the [verified bundled profiles](config.md#model-catalog-and-tools). HTTP acceptance alone does not establish a distinct reasoning level: GLM 5.3 Flash silently treats other strings as `max`. GPT-OSS accepts `low`, `medium`, and `high`. RCP aliases follow the model they currently deploy. When explicit reasoning settings are unknown, Lemex uses the model's default and omits an explicit effort.

Online model commands validate IDs and reasoning before saving. Switching a model clears the previous model's reasoning override. The saved list is stored as `model_picker_models` in `config.toml`; `lemex models reset` restores the bundled defaults without changing the active model. Add `--offline` to use configured metadata without contacting the endpoint; availability is then unknown.
