# API setup

Lemex connects through an API key. It does not offer OpenAI or ChatGPT login.

```sh
lemex setup
```

Setup shows the RCP endpoint (`https://inference.rcp.epfl.ch/v1`) and asks for your API key at a hidden prompt. Press Enter to keep an existing key. It then lets you search the endpoint's models, edit your shortlist, and choose a default model and reasoning. Setup saves the key in the configured credential store (`~/.lemex/auth.json` by default, with user-only permissions on Unix). Set `cli_auth_credentials_store = "keyring"` or `"auto"` in `config.toml` to use the system credential store.

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

If you replace a saved key while an older `LEMEX_API_KEY` is exported, run `unset LEMEX_API_KEY` before starting Lemex (`Remove-Item Env:LEMEX_API_KEY` in PowerShell). Setup's model selection uses the key it just saved; pressing Enter at the hidden key prompt keeps the saved key when one exists.

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
lemex models shortlist                # Edit the shortlist without switching models.
lemex models shortlist MODEL_A MODEL_B # Replace the shortlist.
lemex models shortlist --clear        # Empty it without changing the default.
lemex models reset                    # Restore the provider's initial shortlist.
lemex model                          # Choose and save a model interactively.
lemex model deepseek-ai/DeepSeek-V4.1-Flash-1017 --reasoning max
lemex model --reasoning high          # Change the saved model's reasoning.
lemex model Qwen/Qwen3.5-397B-A17B     # Switch and clear the previous override.
lemex model --reasoning default       # Use the selected model's default.
lemex models --json                   # Machine-readable IDs and capabilities.
```

Inside Lemex, `/model` shows your shortlist. RCP initially uses the six curated
models; a custom provider starts with its configured default. Models not listed
for the current key cannot be selected. Choose **Add model…** and search by name
or ID. Select a result to add or remove it from the shortlist without switching
models. Esc returns to your saved models, where you can choose the model and
reasoning to use as your default.

The search uses the full live catalog available to your key, excluding embedding, reranking, and transcription deployments. A provider refresh preserves the search text. `lemex model` offers the same saved-list and search flow in the terminal. Selecting an available model directly with `lemex model MODEL_ID` also adds it to your list.

Reasoning settings are deployment-specific. Advertised effort settings take
precedence; the RCP provider additionally uses the [verified bundled
profiles](config.md#model-catalog-and-tools). Other providers do not inherit
those profiles. HTTP acceptance alone does not establish a distinct reasoning
level: GLM 5.3 Flash silently treats other strings as `max`. RCP aliases follow
the model they currently deploy. When explicit reasoning settings are unknown,
Lemex uses the deployment default and omits an explicit effort.

Live model listings identify candidates, not verified inference access. Discovery
does not run paid inference probes. For custom provider configuration and
compatibility filtering, see [Change provider](config.md#change-provider).

Online model commands validate IDs and reasoning before saving. Switching a model clears the previous model's reasoning override. The saved list is stored as `model_picker_models` in `config.toml`; `lemex models reset` restores the bundled defaults without changing the active model. Add `--offline` to use configured metadata without contacting the endpoint; availability is then unknown.
