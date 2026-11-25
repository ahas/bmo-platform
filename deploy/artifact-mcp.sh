#!/bin/sh
# The artifact harness plugin, started from the settings folder its service deploys with. The chat's
# standard input and output are the MCP session's; only the process's working folder changes. Its token is
# handed as the file it stands in, read here, since the plugin takes it from its environment alone.
cd "$HOME/.local/share/bmo-artifact/deploy" || exit 1
if [ -z "${BMO_ARTIFACT_SERVER__TOKEN-}" ] && [ -n "${BMO_ARTIFACT_TOKEN_FILE-}" ]; then
  BMO_ARTIFACT_SERVER__TOKEN=$(cat "$BMO_ARTIFACT_TOKEN_FILE") || exit 1
  export BMO_ARTIFACT_SERVER__TOKEN
fi
exec bmo-artifact mcp --address "$1"
