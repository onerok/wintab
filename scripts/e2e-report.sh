#!/usr/bin/env bash
# Generate HTML report by auto-discovering evidence/ directories and screenshots.
# Usage: scripts/e2e-report.sh [test-output-file]
#
# Scans evidence/*/ for directories containing *.png files. Each directory
# becomes a scenario card. Test pass/fail status is matched from cargo test
# output. No hardcoded scenario list — new tests appear automatically.

set -euo pipefail
cd "$(dirname "$0")/.."

RESULTS_FILE="${1:-evidence/test-results.txt}"
REPORT="evidence/report.html"
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')

# ── Collect test results from cargo output ──
declare -A TEST_STATUS
declare -a TEST_NAMES=()
if [[ -f "$RESULTS_FILE" ]]; then
    while IFS= read -r line; do
        if [[ "$line" =~ ^test\ (.+)\ \.\.\.\ (ok|FAILED) ]]; then
            TEST_STATUS["${BASH_REMATCH[1]}"]="${BASH_REMATCH[2]}"
            TEST_NAMES+=("${BASH_REMATCH[1]}")
        fi
    done < "$RESULTS_FILE"
fi

# ── Helpers ──

# Convert snake_case directory name to display title.
# e2e_group_lifecycle → E2E Group Lifecycle
to_title() {
    local s="${1//_/ }"
    # Capitalize first letter of each word
    s=$(echo "$s" | awk '{for(i=1;i<=NF;i++) $i=toupper(substr($i,1,1)) substr($i,2)} 1')
    # Fix common abbreviations that get title-cased wrong
    s="${s//E2e/E2E}"
    s="${s//Ui/UI}"
    s="${s//Api/API}"
    echo "$s"
}

# Convert screenshot filename to step label.
# 01_group_created.png → Group Created
to_step_label() {
    local s="${1%.png}"
    # Strip leading number prefix (01_, 02_, etc.)
    s="${s#[0-9][0-9]_}"
    s="${s//_/ }"
    # Capitalize first letter of each word
    echo "$s" | awk '{for(i=1;i<=NF;i++) $i=toupper(substr($i,1,1)) substr($i,2)} 1'
}

# Find the best matching test name for a directory.
# Returns the test name or empty string if no match.
match_test() {
    local dir="$1"
    # 1. Exact match: acceptance::acceptance_<dir>
    local exact="acceptance::acceptance_${dir}"
    if [[ -v "TEST_STATUS[$exact]" ]]; then
        echo "$exact"
        return
    fi
    # 2. Substring: dir name contained in test name (after prefix)
    for t in "${TEST_NAMES[@]}"; do
        local suffix="${t#acceptance::acceptance_}"
        if [[ "$suffix" == *"$dir"* ]]; then
            echo "$t"
            return
        fi
    done
    # 3. Reverse substring: test suffix contained in dir name
    for t in "${TEST_NAMES[@]}"; do
        local suffix="${t#acceptance::acceptance_}"
        if [[ "$dir" == *"$suffix"* ]]; then
            echo "$t"
            return
        fi
    done
    # 4. Word match: all underscore-delimited words in dir appear in test name
    local best=""
    for t in "${TEST_NAMES[@]}"; do
        local suffix="${t#acceptance::acceptance_}"
        local all_match=true
        IFS='_' read -ra words <<< "$dir"
        for w in "${words[@]}"; do
            if [[ "$suffix" != *"$w"* ]]; then
                all_match=false
                break
            fi
        done
        if $all_match; then
            best="$t"
            break
        fi
    done
    echo "$best"
}

# ── Auto-discover scenarios ──
SCENARIO_DIRS=()
for d in evidence/*/; do
    [[ -d "$d" ]] || continue
    dir_name="$(basename "$d")"
    # Must contain at least one PNG
    shopt -s nullglob
    pngs=("$d"*.png)
    shopt -u nullglob
    if [[ ${#pngs[@]} -gt 0 ]]; then
        SCENARIO_DIRS+=("$dir_name")
    fi
done

# Sort alphabetically
IFS=$'\n' SCENARIO_DIRS=($(sort <<<"${SCENARIO_DIRS[*]}")); unset IFS

# ── Count results for summary ──
pass=0; fail=0; skip=0
for dir in "${SCENARIO_DIRS[@]}"; do
    test_name=$(match_test "$dir")
    if [[ -n "$test_name" ]]; then
        if [[ "${TEST_STATUS[$test_name]}" == "ok" ]]; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
        fi
    else
        skip=$((skip + 1))
    fi
done

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
  .meta { color: var(--dim); font-size: .85rem; margin-bottom: .75rem; }
  .summary { display: flex; gap: 1rem; margin-bottom: 2rem; }
  .summary-stat { font-size: .9rem; font-weight: 600; padding: .3rem .75rem;
                  border-radius: 6px; }
  .summary-pass { background: rgba(63,185,80,.15); color: var(--green); }
  .summary-fail { background: rgba(248,81,73,.15); color: var(--red); }
  .summary-skip { background: rgba(139,148,158,.15); color: var(--dim); }
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

echo "<p class=\"meta\">Generated: ${TIMESTAMP} &middot; ${#SCENARIO_DIRS[@]} scenarios</p>" >> "$REPORT"

# Summary bar
cat >> "$REPORT" <<SUMMARY
<div class="summary">
  <span class="summary-stat summary-pass">${pass} passed</span>
  <span class="summary-stat summary-fail">${fail} failed</span>
  <span class="summary-stat summary-skip">${skip} not run</span>
</div>
SUMMARY

# Lightbox
echo '<div class="lightbox" id="lb" onclick="this.classList.remove('"'"'active'"'"')"><img id="lb-img"></div>' >> "$REPORT"

# ── Emit each scenario ──
for dir in "${SCENARIO_DIRS[@]}"; do
    display_name=$(to_title "$dir")
    test_name=$(match_test "$dir")

    # Badge
    if [[ -n "$test_name" ]]; then
        status="${TEST_STATUS[$test_name]}"
        if [[ "$status" == "ok" ]]; then
            badge_class="badge-pass"; badge_text="PASS"
        else
            badge_class="badge-fail"; badge_text="FAIL"
        fi
        test_label="$test_name"
    else
        badge_class="badge-skip"; badge_text="NOT RUN"
        test_label="(no matching test)"
    fi

    cat >> "$REPORT" <<SCENARIO_HEAD
<div class="scenario">
  <div class="scenario-header">
    <span class="badge ${badge_class}">${badge_text}</span>
    <h2>${display_name}</h2>
    <span style="color:var(--dim);font-size:.8rem;margin-left:auto;">${test_label}</span>
  </div>
  <div class="steps">
SCENARIO_HEAD

    # Discover and sort PNGs
    shopt -s nullglob
    pngs=("evidence/$dir/"*.png)
    shopt -u nullglob

    step_num=1
    for img_path in "${pngs[@]}"; do
        img_file="$(basename "$img_path")"
        rel_img="${dir}/${img_file}"
        step_label=$(to_step_label "$img_file")

        cat >> "$REPORT" <<STEP
    <div class="step">
      <div class="step-title">Step ${step_num}: ${step_label}</div>
      <img src="${rel_img}" alt="${step_label}" onclick="document.getElementById('lb-img').src=this.src;document.getElementById('lb').classList.add('active');">
    </div>
STEP
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

echo "Report generated: ${REPORT} (${#SCENARIO_DIRS[@]} scenarios: ${pass} pass, ${fail} fail, ${skip} not run)"
