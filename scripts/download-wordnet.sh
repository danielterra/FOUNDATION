#!/bin/bash
set -e

# Download WordNet 2024 Edition in RDF/Turtle format
# Source: Open English WordNet (https://en-word.net/)
# License: CC-BY 4.0

WORDNET_VERSION="2024"
WORDNET_URL="https://en-word.net/static/english-wordnet-${WORDNET_VERSION}.ttl.gz"
OUTPUT_DIR="core-ontology"
OUTPUT_FILE="${OUTPUT_DIR}/english-wordnet-${WORDNET_VERSION}.ttl"

echo "Downloading WordNet ${WORDNET_VERSION} Edition..."
echo "URL: ${WORDNET_URL}"

# Create output directory if it doesn't exist
mkdir -p "${OUTPUT_DIR}"

# Download and decompress
curl -L "${WORDNET_URL}" | gunzip > "${OUTPUT_FILE}"

echo "WordNet ${WORDNET_VERSION} downloaded successfully to: ${OUTPUT_FILE}"

# Show file stats
echo ""
echo "File statistics:"
ls -lh "${OUTPUT_FILE}"
echo ""
echo "Lines in file:"
wc -l "${OUTPUT_FILE}"
echo ""
echo "Next step: Run 'npm run build:ontology' to convert to supernova.db"
