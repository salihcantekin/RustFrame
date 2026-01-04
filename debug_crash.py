#!/usr/bin/env python3
import json
import glob
import os

# Find newest crash report
reports = glob.glob(os.path.expanduser("~/Library/Logs/DiagnosticReports/rustframe*.ips"))
if not reports:
    print("No crash reports found")
    exit(1)

newest = max(reports, key=os.path.getmtime)
print(f"Reading: {newest}")

with open(newest) as f:
    content = f.read()
    # IPS files can have multiple JSON documents, take the first one
    # Or they might have a header line before JSON
    lines = content.strip().split('\n')
    # Try to find where JSON starts
    json_start = 0
    for i, line in enumerate(lines):
        if line.strip().startswith('{'):
            json_start = i
            break
    json_content = '\n'.join(lines[json_start:])
    # Handle case where there are multiple JSON docs
    brace_count = 0
    json_end = 0
    for i, char in enumerate(json_content):
        if char == '{':
            brace_count += 1
        elif char == '}':
            brace_count -= 1
            if brace_count == 0:
                json_end = i + 1
                break
    data = json.loads(json_content[:json_end])

# Print exception info
print("\n=== EXCEPTION INFO ===")
if "exception" in data:
    print(f"Exception: {json.dumps(data['exception'], indent=2)}")

if "termination" in data:
    print(f"Termination: {json.dumps(data['termination'], indent=2)}")

if "asi" in data:
    print(f"Application Specific Info: {data['asi']}")

if "lastExceptionBacktrace" in data:
    print(f"Last Exception Backtrace (first 10 frames):")
    for i, frame in enumerate(data["lastExceptionBacktrace"][:10]):
        print(f"  {i}: {json.dumps(frame)}")

# Look for nsexception in any key
print("\n=== NSEXCEPTION SEARCH ===")
for key in data:
    key_lower = key.lower()
    if "nsexception" in key_lower or "reason" in key_lower or "userinfo" in key_lower:
        print(f"{key}: {data[key]}")

# Check threads for triggered thread
print("\n=== TRIGGERED THREAD ===")
if "threads" in data:
    for thread in data["threads"]:
        if thread.get("triggered"):
            print(f"Thread {thread.get('id')} ({thread.get('name', 'unnamed')}) triggered the crash")
            if "frames" in thread:
                print("Stack trace (first 15 frames):")
                for i, frame in enumerate(thread["frames"][:15]):
                    symbol = frame.get("symbol", "???")
                    image = frame.get("imageIndex", "?")
                    offset = frame.get("imageOffset", "?")
                    print(f"  {i}: {symbol} (image {image}, offset {offset})")
            break
