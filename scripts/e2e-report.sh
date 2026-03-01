#!/usr/bin/env bash
# Generate HTML report from E2E test results and screenshot evidence.
# Usage: scripts/e2e-report.sh [test-output-file]
#
# Reads cargo test output (passed via file or stdin) and pairs it with
# screenshots in evidence/ to produce evidence/report.html.

set -euo pipefail
cd "$(dirname "$0")/.."

RESULTS_FILE="${1:-evidence/test-results.txt}"
REPORT="evidence/report.html"
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')

# ── Collect test results from cargo output ──
declare -A TEST_STATUS
if [[ -f "$RESULTS_FILE" ]]; then
    while IFS= read -r line; do
        if [[ "$line" =~ ^test\ (.+)\ \.\.\.\ (ok|FAILED) ]]; then
            name="${BASH_REMATCH[1]}"
            status="${BASH_REMATCH[2]}"
            TEST_STATUS["$name"]="$status"
        fi
    done < "$RESULTS_FILE"
fi

# ── Test scenario definitions ──
# Each scenario: test_name|display_name|evidence_dir|step descriptions
# Steps are semicolon-separated: image_file:expect_lines (pipe-separated expects)
SCENARIOS=(
    "acceptance::acceptance_e2e_group_lifecycle|E2E Group Lifecycle|e2e_group_lifecycle|01_group_created.png:EXPECT: Overlay tab bar visible above the active window|EXPECT: Two tabs displayed in the overlay bar|NOT EXPECT: Windows without overlay;02_tab_switched.png:EXPECT: Tab 0 (first window) now visible|EXPECT: Overlay shows Tab 0 as active|NOT EXPECT: Both windows visible simultaneously;03_ungrouped.png:EXPECT: Both windows visible, no overlay|NOT EXPECT: Overlay tab bar still present"
    "acceptance::acceptance_e2e_minimize_restore|E2E Minimize / Restore|e2e_minimize_restore|01_grouped.png:EXPECT: Overlay tab bar visible above the active window;02_minimized.png:EXPECT: Window minimized, overlay hidden|NOT EXPECT: Overlay tab bar still visible;03_restored.png:EXPECT: Window restored, overlay tab bar visible again|NOT EXPECT: Overlay still hidden"
    "acceptance::acceptance_rules_e2e_auto_group|E2E Rules Auto-Group|rules_auto_group|01_windows_discovered.png:EXPECT: Two DummyApp windows from separate process visible|NOT EXPECT: Overlay tab bar (windows not yet grouped);02_auto_grouped.png:EXPECT: Rules engine grouped both windows, overlay visible|EXPECT: Two tabs in overlay with active tab highlighted;03_tab_switched.png:EXPECT: Tab 0 now active, overlay reflects the switch|NOT EXPECT: Previous tab still showing as active"
    "acceptance::acceptance_group_lifecycle|Group Lifecycle (in-process)|group_lifecycle|01_group_created.png:EXPECT: Overlay tab bar visible above the active window|EXPECT: Two tabs displayed in the overlay bar;02_tab_switched.png:EXPECT: Tab 0 visible, Tab 1 hidden|NOT EXPECT: Both windows visible simultaneously;03_ungrouped.png:EXPECT: Both windows visible, no overlay|NOT EXPECT: Overlay tab bar still present"
    "acceptance::acceptance_minimize_restore_group|Minimize / Restore (in-process)|minimize_restore|01_minimized.png:EXPECT: Active window minimized to taskbar|NOT EXPECT: Overlay tab bar still visible;02_restored.png:EXPECT: Window restored, overlay tab bar visible again|NOT EXPECT: Overlay still hidden"
)

# ── Generate HTML ──
cat > "$REPORT" <<'HEADER'
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>WinTab E2E Test Report</title>
<style>
  :root { --bg: #0d1117; --card: #161b22; --border: #30363d; --text: #e6edf3;
          --green: #3fb950; --red: #f85149; --dim: #8b949e; --accent: #58a6ff; }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { font-family: -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif;
         background: var(--bg); color: var(--text); padding: 2rem; line-height: 1.5; }
  h1 { font-size: 1.6rem; margin-bottom: .25rem; }
  .meta { color: var(--dim); font-size: .85rem; margin-bottom: 2rem; }
  .scenario { background: var(--card); border: 1px solid var(--border);
              border-radius: 8px; margin-bottom: 2rem; overflow: hidden; }
  .scenario-header { display: flex; align-items: center; gap: .75rem;
                     padding: 1rem 1.25rem; border-bottom: 1px solid var(--border); }
  .scenario-header h2 { font-size: 1.1rem; font-weight: 600; }
  .badge { font-size: .75rem; font-weight: 600; padding: .15rem .5rem;
           border-radius: 12px; text-transform: uppercase; letter-spacing: .03em; }
  .badge-pass { background: rgba(63,185,80,.15); color: var(--green); border: 1px solid rgba(63,185,80,.3); }
  .badge-fail { background: rgba(248,81,73,.15); color: var(--red); border: 1px solid rgba(248,81,73,.3); }
  .badge-skip { background: rgba(139,148,158,.15); color: var(--dim); border: 1px solid rgba(139,148,158,.3); }
  .steps { display: flex; flex-wrap: wrap; gap: 1.25rem; padding: 1.25rem; }
  .step { flex: 1 1 320px; min-width: 280px; max-width: 520px; }
  .step-title { font-weight: 600; font-size: .9rem; margin-bottom: .5rem; color: var(--accent); }
  .step img { width: 100%; border-radius: 6px; border: 1px solid var(--border);
              margin-bottom: .5rem; cursor: pointer; transition: transform .15s; }
  .step img:hover { transform: scale(1.02); }
  .expects { list-style: none; font-size: .82rem; }
  .expects li { padding: .15rem 0; padding-left: 1.2em; text-indent: -1.2em; }
  .expects li::before { margin-right: .35em; }
  .expect-yes::before { content: "\2705"; }
  .expect-no::before  { content: "\274C"; }
  .no-evidence { color: var(--dim); font-style: italic; padding: 1.25rem; }
  /* lightbox */
  .lightbox { display: none; position: fixed; inset: 0; background: rgba(0,0,0,.85);
              z-index: 100; justify-content: center; align-items: center; cursor: zoom-out; }
  .lightbox.active { display: flex; }
  .lightbox img { max-width: 95vw; max-height: 95vh; border-radius: 6px; }
</style>
</head>
<body>
<h1>WinTab E2E Test Report</h1>
HEADER

echo "<p class=\"meta\">Generated: ${TIMESTAMP}</p>" >> "$REPORT"

# Lightbox container
echo '<div class="lightbox" id="lb" onclick="this.classList.remove('"'"'active'"'"')"><img id="lb-img"></div>' >> "$REPORT"

for scenario in "${SCENARIOS[@]}"; do
    IFS='|' read -r test_name display_name evidence_dir steps_raw <<< "$scenario"

    # Determine status
    status="${TEST_STATUS[$test_name]:-skip}"
    if [[ "$status" == "ok" ]]; then
        badge_class="badge-pass"
        badge_text="PASS"
    elif [[ "$status" == "FAILED" ]]; then
        badge_class="badge-fail"
        badge_text="FAIL"
    else
        badge_class="badge-skip"
        badge_text="NOT RUN"
    fi

    cat >> "$REPORT" <<SCENARIO_HEAD
<div class="scenario">
  <div class="scenario-header">
    <span class="badge ${badge_class}">${badge_text}</span>
    <h2>${display_name}</h2>
    <span style="color:var(--dim);font-size:.8rem;margin-left:auto;">${test_name}</span>
  </div>
  <div class="steps">
SCENARIO_HEAD

    # Parse steps (semicolon-separated)
    IFS=';' read -ra step_list <<< "$steps_raw"
    step_num=1
    for step_entry in "${step_list[@]}"; do
        # First token before : is the image filename, rest are expect lines (pipe-separated)
        img_file="${step_entry%%:*}"
        expects_raw="${step_entry#*:}"
        img_path="evidence/${evidence_dir}/${img_file}"
        # Relative path from report location
        rel_img="${evidence_dir}/${img_file}"

        step_label="${img_file%.png}"
        step_label="${step_label//_/ }"

        echo "    <div class=\"step\">" >> "$REPORT"
        echo "      <div class=\"step-title\">Step ${step_num}: ${step_label}</div>" >> "$REPORT"

        if [[ -f "$img_path" ]]; then
            echo "      <img src=\"${rel_img}\" alt=\"${step_label}\" onclick=\"document.getElementById('lb-img').src=this.src;document.getElementById('lb').classList.add('active');\">" >> "$REPORT"
        else
            echo "      <div class=\"no-evidence\">Screenshot not found: ${rel_img}</div>" >> "$REPORT"
        fi

        echo "      <ul class=\"expects\">" >> "$REPORT"
        IFS='|' read -ra expect_lines <<< "$expects_raw"
        for exp in "${expect_lines[@]}"; do
            if [[ "$exp" == NOT\ EXPECT:* ]]; then
                echo "        <li class=\"expect-no\">${exp}</li>" >> "$REPORT"
            else
                echo "        <li class=\"expect-yes\">${exp}</li>" >> "$REPORT"
            fi
        done
        echo "      </ul>" >> "$REPORT"
        echo "    </div>" >> "$REPORT"

        step_num=$((step_num + 1))
    done

    echo "  </div>" >> "$REPORT"
    echo "</div>" >> "$REPORT"
done

cat >> "$REPORT" <<'FOOTER'
<script>
document.addEventListener('keydown', e => {
  if (e.key === 'Escape') document.getElementById('lb').classList.remove('active');
});
</script>
</body>
</html>
FOOTER

echo "Report generated: ${REPORT}"
