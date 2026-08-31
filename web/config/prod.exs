import Config

# Compile-time prod overrides. Imports the shared base so
# `mix deps.compile` (MIX_ENV=prod) and the release both see it;
# runtime.exs is evaluated separately by the release config provider.
import_config "config.exs"
