#!/bin/bash
set -euo pipefail

PLUGIN_NAME="sel2ical"
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
TEMP_DIR=$(mktemp -d)

# Copy files to temp directory
cp "$SCRIPT_DIR/manifest.json" "$SCRIPT_DIR/icon.png" "$SCRIPT_DIR/background.js" "$SCRIPT_DIR/content.js" "$TEMP_DIR/"

set -a
./.env
# Process each JavaScript file
for file in "$TEMP_DIR"/*.js "$TEMP_DIR/manifest.json"; do
    python3 "$SCRIPT_DIR/replace_env.py" "$file"
done
set +a

# Create the ZIP file
pushd "$TEMP_DIR" > /dev/null
zip -r "${PLUGIN_NAME}.zip" .
popd > /dev/null

# Rename the ZIP file to XPI and move it to the script directory
mv "$TEMP_DIR/${PLUGIN_NAME}.zip" "$SCRIPT_DIR/${PLUGIN_NAME}.xpi"

# Clean up the temporary directory
rm -rf "$TEMP_DIR"

echo "Plugin packaged as ${SCRIPT_DIR}/${PLUGIN_NAME}.xpi"
