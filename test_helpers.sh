#!/bin/bash
# Test script for download helper functions

echo "=== Testing apkeep helper functions ==="
echo ""

# Test 1: Download an APK
echo "Test 1: Downloading APK..."
apkeep -a com.mhss.app.mybrain@3.0.1 -d f-droid /tmp
if [ -f "/tmp/com.mhss.app.mybrain@3.0.1.apk" ]; then
    echo "✓ APK downloaded successfully"
    APK_FILE="/tmp/com.mhss.app.mybrain@3.0.1.apk"
else
    echo "✗ APK download failed"
    exit 1
fi
echo ""

# Test 2: Compute SHA256 checksum
echo "Test 2: Computing SHA256 checksum..."
CHECKSUM=$(sha256sum "$APK_FILE" | awk '{print $1}')
echo "✓ Checksum: $CHECKSUM"
echo ""

# Test 3: Test custom headers
echo "Test 3: Testing custom User-Agent..."
apkeep -a com.zhiliaoapp.musically -d huawei-app-gallery --user-agent "TestBot/1.0" /tmp 2>&1 | head -5
echo ""

# Test 4: Test with custom headers
echo "Test 4: Testing custom headers..."
apkeep -a com.instagram.android --headers "Accept:application/json,X-Test:value" /tmp 2>&1 | head -5
echo ""

# Test 5: List versions (tests API connectivity)
echo "Test 5: Listing versions..."
apkeep -l -a com.mhss.app.mybrain -d f-droid
echo ""

echo "=== All tests completed ==="
