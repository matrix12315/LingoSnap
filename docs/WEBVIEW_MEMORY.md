# Fair-copy WebView2 memory report

This report covers the memory of the fair-copy UI companion (`lingosnap-ui.exe`) and the
WebView2 processes it starts. The resident (`lingosnap-resident.exe`) is
not changed by this work. Its 20 MiB idle requirement applies with the classic skin
and with the fair-copy companion stopped.

## How it was measured

- Machine: NVIDIA GeForce MX450, one 2560×1600 display at 200 % scale, WebView2
  Runtime 157.0.4341.42.
- Build: release `lingosnap-ui.exe`, driven over its named pipe by
  `windows/tmp/mem-probe.ps1`, with a separate WebView2 user-data folder
  (`LOCALAPPDATA=windows/tmp/lad`).
- Metric: private working set (the Task Manager "Memory" column), summed over
  `lingosnap-ui.exe` and every descendant (`msedgewebview2.exe` browser, GPU,
  renderer, network, storage and crashpad processes). Commit (private bytes) is
  also recorded.
- Scenario, in order: warm idle (pool window preloaded, nothing shown), one
  result card shown, three cards shown at once, all closed (+5 s), all closed
  (+30 s), and one card shown again.
- Manager: `windows/tmp/mgr-mem.ps1` opens the manager window and measures its
  process tree 10 s later. The r61 number uses the packaged r61 build; the r62
  number uses the debug build, where only the small host process differs from
  release.

## Results

Private working set in MiB (commit in brackets):

| State | Before (r61) | After (r62) | Change |
|---|---:|---:|---:|
| Warm idle | 297.4 (359.2) | 51.0 (118.5) | −83 % |
| One card shown | 251.5 (310.8) | 87.2 (133.3) | −65 % |
| Three cards shown | 563.5 (649.3) | 226.2 (298.1) | −60 % |
| All closed, +5 s | 513.0 (586.7) | 61.1 (124.4) | −88 % |
| All closed, +30 s | 419.4 (490.2) | 18.1 (120.6) | −96 % |
| Card shown again | 420.6 (491.5) | 55.2 (142.5) | −87 % |
| Manager window open | 215.0 | 103.5 | −52 % |
| No popup for 10 min | 419 (stays) | 0 (companion exits) | −100 % |

In r61 the GPU process alone held 237 MiB at warm idle and 452 MiB with three
cards. In r62 it holds 15 MiB and 102 MiB.

## What changed and what each change saved

Each change was measured on its own before it was combined with the others.

1. **Viewport sized to the window.** Since r58, every popup's WebView viewport was
   the size of the whole monitor work area. That stopped flashes, but GPU memory
   grows with viewport area. The viewport is now the window plus a margin of
   32 × 240 CSS px. It grows before the window does, so a growing card still
   reveals pixels that are already painted. The work area remains the upper
   limit.
2. **Pooled windows are parked.** A hidden pool window stops compositing
   (`SetIsVisible(false)`) and asks the browser for the low memory target
   (`ICoreWebView2_19::SetMemoryUsageTargetLevel(LOW)`). It returns to visible
   and the normal target as soon as a popup takes it. Together with change 1:
   warm idle 297 → 143 MiB, and all closed +30 s 419 → 21 MiB.
3. **Pool of one window instead of two.** One preloaded window is enough for the
   next popup to open without a page load. Each extra window kept a renderer
   (about 20 MiB) and its GPU surfaces.
4. **Software rendering (`--disable-gpu`).** Most of the remaining memory was in
   the GPU process: about 150 MiB per visible card at 200 % scale. That cost
   comes from the transparent DirectComposition surfaces and the per-element SVG
   backdrop filters (the liquid-glass lens). Removing just the lens filters
   saved only 32 MiB, so the lens alone did not explain the cost. Software
   rendering cut one shown card from 201 to 87 MiB, and screenshots of the
   popup and manager match the GPU output: rounded transparent corners, glass,
   lens and shadows.
5. **Browser background services off.** The pages are local strings and never
   use the network. These switches are now off: SmartScreen, the spare
   renderer, Translate, optimization hints, autofill server traffic, media
   router, background networking, component update, sync and pings. They saved
   about 3 MiB in the browser process and 1 MiB in the network service.
6. **Idle exit after use.** The 600 s idle exit used to apply only to a companion
   that had never shown anything. Once a popup had been shown, the companion
   stayed resident for good. The timer now starts again whenever the last popup
   closes. After 10 minutes with nothing on screen, the companion and all its
   browser processes exit. The resident already restarts the companion on the
   next fair-copy popup (`ensure_companion` checks `child_exited`). That first
   popup pays a cold WebView2 start.

Flags that were tried and dropped, because they saved little or cost more
memory:

| Flag | One card shown | Three shown | Verdict |
|---|---:|---:|---|
| (none, after changes 1–3) | 201.2 | 409.8 | baseline |
| `--force-gpu-mem-available-mb=32` | 191.1 | 374.2 | small gain |
| `--enable-low-end-device-mode` | 189.9 | 362.2 | small gain; may lower image quality |
| `--disable-gpu-rasterization` | 235.2 | 584.3 | worse |
| `--renderer-process-limit=1` | 259.6 | 595.7 | no effect (one renderer per window remains) |
| `--disable-gpu` | 88.5 | 189.2 | **adopted** |

## Costs and trade-offs

- **CPU while a card streams.** 60 deltas at 50 ms intervals (about 6 s) cost
  1.6 s of CPU with GPU rendering and 4.0–4.25 s with software rendering, summed
  over all processes. About a third of the software cost is the lens filter
  (2.6 s without it). A card that is shown but idle uses no measurable CPU in
  either mode.
- **Flashes.** A screenshot sampler checked the chooser, a card that grows over
  30 deltas, and a parked window being reused. It found no black or white
  frames.
- **First popup after 10 idle minutes.** The companion starts cold, so that one
  popup appears later than usual.
- **Opting back into GPU rendering.** Set `LINGOSNAP_WEBVIEW_GPU=1` before
  starting the resident. Setting `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`
  replaces all of the companion's browser switches; the dev probes use it for
  `--remote-debugging-port`, so they run with GPU rendering unless they add
  `--disable-gpu` themselves.

## Reproduce

```powershell
# release build of the companion
.\windows\tmp\vscargo.ps1 build --release --package lingosnap-ui
# popup scenario (pass -BrowserArgs "..." to try other switches)
powershell -NoProfile -ExecutionPolicy Bypass -File windows\tmp\mem-probe.ps1
# manager window
powershell -NoProfile -ExecutionPolicy Bypass -File windows\tmp\mgr-mem.ps1 -Exe <path to lingosnap-manager.exe>
```

The probe scripts live in `windows/tmp/`, which is scratch space and is not
committed. The numbers above come from runs made on 2026-11-10.
