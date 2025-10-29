#!/bin/bash
# Quick start script for nostr.blue Rust Edition

echo "🦀 nostr.blue (Rust Edition) - Quick Start"
echo ""
echo "📦 Checking dependencies..."

# Check if trunk is installed
if ! command -v trunk &> /dev/null; then
    echo "❌ Trunk not found. Installing..."
    cargo install trunk wasm-bindgen-cli
else
    echo "✅ Trunk installed"
fi

# Check if node/npm is installed
if ! command -v npm &> /dev/null; then
    echo "⚠️  npm not found. Please install Node.js for TailwindCSS"
    exit 1
else
    echo "✅ npm installed"
fi

echo ""
echo "📦 Installing npm dependencies..."
if [ -f "package-lock.json" ]; then
    npm ci
else
    npm install
fi

echo ""
echo "🎨 Building CSS..."
npm run tailwind:build

echo ""
echo "🚀 Starting development server..."
echo ""
echo "📱 App will be available at: http://localhost:8080"
echo "🔄 Hot reload enabled - changes will auto-refresh"
echo ""
echo "Press Ctrl+C to stop the server"
echo ""

trunk serve
