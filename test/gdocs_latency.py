"""
gdocs_latency.py
================
Measures Google Docs change-propagation latency using two Playwright
browser instances — one writer and one reader — both logged in as real
Google accounts.

How it works
------------
1. Writer browser navigates to the Doc and types a unique sentinel string.
2. The reader browser (already open on the same Doc) polls the DOM for
   the sentinel to appear.
3. The delta between keystroke completion and DOM detection is the
   propagation latency.
4. RTT to Google servers is estimated by timing fetch() calls to the
   Docs page from inside the browser context.

Setup
-----
    pip install playwright
    playwright install chromium        # or: npx playwright install chromium

    # On Arch/CachyOS without pip:
    sudo pacman -S python-playwright
    python -m playwright install chromium

Run
---
    python gdocs_latency.py --doc-url "https://docs.google.com/document/d/YOUR_ID/edit" --rounds 15

On first run you will be prompted to log in manually in each browser
window. After login, sessions are saved to writer_session/ and
reader_session/ so subsequent runs skip the login step.

Notes
-----
- Use TWO different Google accounts (writer_email vs reader_email) so
  that the reader is a genuine remote client, not the same session.
- Both accounts must have at least view access to the document;
  give the reader account edit access if you want it to confirm via edits.
- Propagation time includes the full collaborative pipeline:
  writer keystroke → Docs OT → Google servers → reader DOM update.
"""

import argparse
import asyncio
import statistics
import time
import json
import re
from datetime import datetime, timezone
from playwright.async_api import async_playwright

# ── Configuration ─────────────────────────────────────────────────────────────

WRITER_SESSION_DIR = "./writer_session"
READER_SESSION_DIR = "./reader_session"

POLL_INTERVAL_MS   = 100   # how often the reader checks the DOM (milliseconds)
SENTINEL_PREFIX    = "PROBE_"
TIMEOUT_S          = 30    # give up after this many seconds per round

# ── RTT helper ────────────────────────────────────────────────────────────────

RTT_SCRIPT = """
async () => {
    const t0 = performance.now();
    await fetch(location.href, { method: 'HEAD', cache: 'no-store' });
    return performance.now() - t0;
}
"""

async def measure_rtt(page, samples: list, n: int = 3):
    for _ in range(n):
        ms = await page.evaluate(RTT_SCRIPT)
        samples.append(ms)

# ── Writer ────────────────────────────────────────────────────────────────────

async def write_sentinel(page, sentinel: str) -> float:
    """
    Click into the document body and type the sentinel.
    Returns the perf_counter timestamp immediately after the last keystroke.
    """
    # Click somewhere safe in the document body
    await page.locator(".kix-appview-editor").click()
    # Move to end of document
    await page.keyboard.press("Control+End")
    await page.keyboard.press("Enter")
    await page.keyboard.type(sentinel, delay=0)
    t_sent = time.perf_counter()
    return t_sent

# ── Reader ────────────────────────────────────────────────────────────────────

DETECT_SCRIPT = """
(sentinel) => {
    const spans = document.querySelectorAll('.kix-wordhtmlgenerator-word-node');
    for (const s of spans) {
        if (s.textContent.includes(sentinel)) return true;
    }
    // fallback: search full editor text
    const editor = document.querySelector('.kix-appview-editor');
    return editor ? editor.innerText.includes(sentinel) : false;
}
"""

async def wait_for_sentinel(page, sentinel: str, poll_ms: int, timeout_s: float):
    """
    Poll the reader DOM until the sentinel appears.
    Returns detection timestamp (perf_counter) or None on timeout.
    """
    deadline = time.perf_counter() + timeout_s
    while time.perf_counter() < deadline:
        found = await page.evaluate(DETECT_SCRIPT, sentinel)
        if found:
            return time.perf_counter()
        await asyncio.sleep(poll_ms / 1000)
    return None

# ── Cleanup ───────────────────────────────────────────────────────────────────

async def delete_last_line(page):
    """Select and delete the sentinel line in the writer window."""
    await page.locator(".kix-appview-editor").click()
    await page.keyboard.press("Control+End")
    await page.keyboard.press("Home")
    await page.keyboard.press("Shift+End")
    await page.keyboard.press("Shift+Delete")
    await page.keyboard.press("Backspace")   # remove the blank line

# ── Login flow ────────────────────────────────────────────────────────────────

async def ensure_logged_in(context, doc_url: str, role: str):
    page = await context.new_page()
    await page.goto(doc_url)
    # If redirected to accounts.google.com the user needs to log in
    if "accounts.google.com" in page.url or "ServiceLogin" in page.url:
        print(f"\n[{role}] Not logged in. Please sign in in the browser window that just opened.")
        print(f"[{role}] Press ENTER here once you are back on the Google Doc…")
        input()
        await page.goto(doc_url)
    await page.wait_for_load_state("networkidle", timeout=20_000)
    return page

# ── Main ──────────────────────────────────────────────────────────────────────

async def run(doc_url: str, rounds: int, poll_ms: int):
    propagation_samples = []
    rtt_samples = []

    async with async_playwright() as pw:
        # --- open two persistent contexts (separate sessions / accounts) ----
        writer_ctx = await pw.chromium.launch_persistent_context(
            WRITER_SESSION_DIR,
            headless=False,
            args=["--disable-blink-features=AutomationControlled"],
        )
        reader_ctx = await pw.chromium.launch_persistent_context(
            READER_SESSION_DIR,
            headless=False,
            args=["--disable-blink-features=AutomationControlled"],
        )

        print("Opening writer browser…")
        writer_page = await ensure_logged_in(writer_ctx, doc_url, "WRITER")
        print("Opening reader browser…")
        reader_page = await ensure_logged_in(reader_ctx, doc_url, "READER")

        print(f"\nBoth browsers open. Starting {rounds} measurement rounds.\n")
        await asyncio.sleep(2)   # let both editors fully settle

        for i in range(1, rounds + 1):
            sentinel = f"{SENTINEL_PREFIX}{datetime.now(timezone.utc).strftime('%H%M%S%f')}"
            print(f"Round {i}/{rounds}  sentinel={sentinel} … ", end="", flush=True)

            # Measure RTT from the reader side
            await measure_rtt(reader_page, rtt_samples, n=2)

            # Write
            t_write = await write_sentinel(writer_page, sentinel)

            # Detect
            t_detect = await wait_for_sentinel(reader_page, sentinel, poll_ms, TIMEOUT_S)

            if t_detect is not None:
                latency_ms = (t_detect - t_write) * 1000
                propagation_samples.append(latency_ms)
                print(f"{latency_ms:.1f} ms")
            else:
                print("TIMEOUT")

            # Clean up sentinel from the document
            await delete_last_line(writer_page)
            await asyncio.sleep(1.5)   # pause between rounds

        await writer_ctx.close()
        await reader_ctx.close()

    # ── Summary ───────────────────────────────────────────────────────────────
    print("\n" + "═" * 52)
    print("PROPAGATION LATENCY  (keystroke → reader DOM)")
    print("═" * 52)
    if propagation_samples:
        print(f"  Rounds completed : {len(propagation_samples)}/{rounds}")
        print(f"  Min              : {min(propagation_samples):.1f} ms")
        print(f"  Max              : {max(propagation_samples):.1f} ms")
        print(f"  Mean             : {statistics.mean(propagation_samples):.1f} ms")
        print(f"  Median           : {statistics.median(propagation_samples):.1f} ms")
        if len(propagation_samples) > 1:
            print(f"  Std dev          : {statistics.stdev(propagation_samples):.1f} ms")
    else:
        print("  No successful measurements.")

    print("\n" + "═" * 52)
    print("SERVER RTT  (HEAD fetch from reader browser)")
    print("═" * 52)
    if rtt_samples:
        print(f"  Samples          : {len(rtt_samples)}")
        print(f"  Min              : {min(rtt_samples):.1f} ms")
        print(f"  Max              : {max(rtt_samples):.1f} ms")
        print(f"  Mean             : {statistics.mean(rtt_samples):.1f} ms")
        print(f"  Median           : {statistics.median(rtt_samples):.1f} ms")
        if len(rtt_samples) > 1:
            print(f"  Std dev          : {statistics.stdev(rtt_samples):.1f} ms")

    results = {
        "propagation_ms": propagation_samples,
        "rtt_ms": rtt_samples,
        "config": {"rounds": rounds, "poll_interval_ms": poll_ms},
    }
    out_file = f"results_{datetime.now().strftime('%Y%m%d_%H%M%S')}.json"
    with open(out_file, "w") as f:
        json.dump(results, f, indent=2)
    print(f"\nRaw results saved to {out_file}")


def main():
    parser = argparse.ArgumentParser(description="Measure Google Docs propagation latency via two browser clients.")
    parser.add_argument("--doc-url", required=True, help='Full URL of the Google Doc, e.g. "https://docs.google.com/document/d/ID/edit"')
    parser.add_argument("--rounds",  type=int, default=10, help="Number of measurement rounds (default: 10)")
    parser.add_argument("--poll-ms", type=int, default=POLL_INTERVAL_MS, help=f"Reader DOM poll interval in ms (default: {POLL_INTERVAL_MS})")
    args = parser.parse_args()
    asyncio.run(run(args.doc_url, args.rounds, args.poll_ms))


if __name__ == "__main__":
    main()
