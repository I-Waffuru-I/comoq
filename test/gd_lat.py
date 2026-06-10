"""
gdocs_latency.py
================
Measures how long it takes for a text change made via the Google Docs API
to become visible to a second polling client, simulating propagation latency.

Also tracks RTT to Google's servers by timing each API call.

Setup
-----
1. Install dependencies:
       pip install google-api-python-client google-auth-httplib2 google-auth-oauthlib

2. Enable the Google Docs API and Google Drive API in Google Cloud Console.

3. Create OAuth 2.0 credentials (Desktop app), download as `credentials.json`
   in the same directory as this script.

4. Set DOCUMENT_ID below to an existing Google Doc you own.

Usage
-----
    python gdocs_latency.py --rounds 20 --interval 0.5

The script will:
  - Insert a timestamped sentinel string into the document (writer role).
  - Continuously poll the document content until it detects the change (reader role).
  - Record the time delta as propagation latency.
  - Record raw API call durations as RTT samples.
  - Print a summary at the end.
"""

import argparse
import time
import statistics
import json
from datetime import datetime, timezone

from google.oauth2.credentials import Credentials
from google_auth_oauthlib.flow import InstalledAppFlow
from google.auth.transport.requests import Request
from googleapiclient.discovery import build
import os
import pickle

# ── Configuration ─────────────────────────────────────────────────────────────

DOCUMENT_ID = "YOUR_DOCUMENT_ID_HERE"  # <-- replace with your Doc ID
SCOPES = ["https://www.googleapis.com/auth/documents"]
TOKEN_FILE = "token.pickle"
CREDENTIALS_FILE = "credentials.json"

# ── Auth ───────────────────────────────────────────────────────────────────────


def get_service():
    creds = None
    if os.path.exists(TOKEN_FILE):
        with open(TOKEN_FILE, "rb") as f:
            creds = pickle.load(f)

    if not creds or not creds.valid:
        if creds and creds.expired and creds.refresh_token:
            creds.refresh(Request())
        else:
            flow = InstalledAppFlow.from_client_secrets_file(CREDENTIALS_FILE, SCOPES)
            creds = flow.run_local_server(port=0)
        with open(TOKEN_FILE, "wb") as f:
            pickle.dump(creds, f)

    return build("docs", "v1", credentials=creds)


# ── Core helpers ───────────────────────────────────────────────────────────────


def timed_api_call(fn, rtt_samples: list):
    """Execute fn(), record wall-clock duration as RTT sample, return result."""
    t0 = time.perf_counter()
    result = fn()
    elapsed_ms = (time.perf_counter() - t0) * 1000
    rtt_samples.append(elapsed_ms)
    return result, elapsed_ms


def get_doc_text(service, rtt_samples: list) -> str:
    """Fetch the full plain text of the document."""
    doc, _ = timed_api_call(
        lambda: service.documents().get(documentId=DOCUMENT_ID).execute(),
        rtt_samples,
    )
    text_parts = []
    for element in doc.get("body", {}).get("content", []):
        para = element.get("paragraph")
        if para:
            for run in para.get("elements", []):
                t = run.get("textRun", {}).get("content", "")
                text_parts.append(t)
    return "".join(text_parts)


def insert_sentinel(service, sentinel: str, rtt_samples: list):
    """Append a sentinel string at the end of the document body."""
    # First fetch end index
    doc, _ = timed_api_call(
        lambda: service.documents().get(documentId=DOCUMENT_ID).execute(),
        rtt_samples,
    )
    content = doc.get("body", {}).get("content", [])
    # End index of body (last structural element)
    end_index = content[-1].get("endIndex", 1) - 1

    requests = [
        {
            "insertText": {
                "location": {"index": end_index},
                "text": sentinel + "\n",
            }
        }
    ]
    timed_api_call(
        lambda: (
            service.documents()
            .batchUpdate(documentId=DOCUMENT_ID, body={"requests": requests})
            .execute()
        ),
        rtt_samples,
    )


def cleanup_sentinel(service, sentinel: str, rtt_samples: list):
    """Remove the sentinel line from the document."""
    text = get_doc_text(service, rtt_samples)
    idx = text.find(sentinel)
    if idx == -1:
        return
    # +1 for Docs API 1-based indexing
    start = idx + 1
    end = start + len(sentinel) + 1  # +1 for the \n we appended
    requests = [
        {"deleteContentRange": {"range": {"startIndex": start, "endIndex": end}}}
    ]
    timed_api_call(
        lambda: (
            service.documents()
            .batchUpdate(documentId=DOCUMENT_ID, body={"requests": requests})
            .execute()
        ),
        rtt_samples,
    )


# ── Measurement loop ───────────────────────────────────────────────────────────


def measure_propagation(
    service, poll_interval: float, rtt_samples: list
) -> float | None:
    """
    Insert a unique sentinel, then poll until it is visible.
    Returns propagation latency in milliseconds, or None on timeout.
    """
    sentinel = f"__LATENCY_PROBE_{datetime.now(timezone.utc).isoformat()}__"
    timeout = 30.0  # seconds

    # Write
    insert_sentinel(service, sentinel, rtt_samples)
    t_write = time.perf_counter()

    # Poll until visible
    deadline = t_write + timeout
    while time.perf_counter() < deadline:
        time.sleep(poll_interval)
        content = get_doc_text(service, rtt_samples)
        if sentinel in content:
            latency_ms = (time.perf_counter() - t_write) * 1000
            cleanup_sentinel(service, sentinel, rtt_samples)
            return latency_ms

    print("  [!] Timeout: sentinel not detected within 30 s")
    cleanup_sentinel(service, sentinel, rtt_samples)
    return None


# ── Main ───────────────────────────────────────────────────────────────────────


def main():
    parser = argparse.ArgumentParser(
        description="Measure Google Docs change propagation latency."
    )
    parser.add_argument(
        "--rounds",
        type=int,
        default=10,
        help="Number of measurement rounds (default: 10)",
    )
    parser.add_argument(
        "--interval",
        type=float,
        default=0.25,
        help="Poll interval in seconds (default: 0.25)",
    )
    parser.add_argument(
        "--output",
        type=str,
        default=None,
        help="Optional JSON file to save raw results",
    )
    args = parser.parse_args()

    if DOCUMENT_ID == "YOUR_DOCUMENT_ID_HERE":
        print("ERROR: Set DOCUMENT_ID at the top of the script before running.")
        return

    print(f"Connecting to Google Docs API…")
    service = get_service()
    print(
        f"Connected. Running {args.rounds} rounds (poll interval: {args.interval}s)\n"
    )

    propagation_samples = []
    rtt_samples = []

    for i in range(1, args.rounds + 1):
        print(f"Round {i}/{args.rounds}… ", end="", flush=True)
        latency = measure_propagation(service, args.interval, rtt_samples)
        if latency is not None:
            propagation_samples.append(latency)
            print(f"{latency:.1f} ms")
        else:
            print("skipped (timeout)")
        time.sleep(1.0)  # brief pause between rounds

    # ── Summary ───────────────────────────────────────────────────────────────
    print("\n" + "═" * 50)
    print("PROPAGATION LATENCY  (write → read detection)")
    print("═" * 50)
    if propagation_samples:
        print(f"  Rounds completed : {len(propagation_samples)}/{args.rounds}")
        print(f"  Min              : {min(propagation_samples):.1f} ms")
        print(f"  Max              : {max(propagation_samples):.1f} ms")
        print(f"  Mean             : {statistics.mean(propagation_samples):.1f} ms")
        print(f"  Median           : {statistics.median(propagation_samples):.1f} ms")
        if len(propagation_samples) > 1:
            print(
                f"  Std dev          : {statistics.stdev(propagation_samples):.1f} ms"
            )
    else:
        print("  No successful measurements.")

    print("\n" + "═" * 50)
    print("API ROUND-TRIP TIME  (raw per-call wall clock)")
    print("═" * 50)
    if rtt_samples:
        print(f"  Samples          : {len(rtt_samples)}")
        print(f"  Min              : {min(rtt_samples):.1f} ms")
        print(f"  Max              : {max(rtt_samples):.1f} ms")
        print(f"  Mean             : {statistics.mean(rtt_samples):.1f} ms")
        print(f"  Median           : {statistics.median(rtt_samples):.1f} ms")
        if len(rtt_samples) > 1:
            print(f"  Std dev          : {statistics.stdev(rtt_samples):.1f} ms")

    if args.output:
        data = {
            "propagation_ms": propagation_samples,
            "rtt_ms": rtt_samples,
            "config": {"rounds": args.rounds, "poll_interval_s": args.interval},
        }
        with open(args.output, "w") as f:
            json.dump(data, f, indent=2)
        print(f"\nRaw data saved to {args.output}")


if __name__ == "__main__":
    main()
