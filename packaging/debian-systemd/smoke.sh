#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "${WORK_DIR}"' EXIT

PROFILE_SOURCE="${WORK_DIR}/production-profile"
DESTDIR="${WORK_DIR}/destdir"
DAEMON_BIN="${WORK_DIR}/bin/aegisai-runtime-daemon"
HELPER_BIN="${WORK_DIR}/bin/aegisai-ebpf-helper"

mkdir -p \
  "${PROFILE_SOURCE}/classifier" \
  "${PROFILE_SOURCE}/scenarios" \
  "${PROFILE_SOURCE}/safety" \
  "${WORK_DIR}/bin"

cp "${REPO_ROOT}/configs/runtime/runtime.example.toml" "${PROFILE_SOURCE}/runtime.toml"
cp "${REPO_ROOT}/configs/classifier/process_rules.example.toml" \
  "${PROFILE_SOURCE}/classifier/process_rules.toml"
cp "${REPO_ROOT}/configs/scenarios/ai_workload_awareness.example.toml" \
  "${PROFILE_SOURCE}/scenarios/ai_workload_awareness.toml"
cp "${REPO_ROOT}/configs/scenarios/inference_tail_guard.example.toml" \
  "${PROFILE_SOURCE}/scenarios/inference_tail_guard.toml"
cp "${REPO_ROOT}/configs/scenarios/tool_call_booster.example.toml" \
  "${PROFILE_SOURCE}/scenarios/tool_call_booster.toml"
cp "${REPO_ROOT}/configs/safety/default.toml" "${PROFILE_SOURCE}/safety/default.toml"

cat >"${PROFILE_SOURCE}/runtime.toml" <<'PROFILE'
[target]
deployment_target = "linux"
kernel_min = "5.15"
cgroup_version = "v2"

[runtime]
primary_runtime = "ollama"
fallback_runtime = "llama.cpp"

[selection]
mode = "pid_allowlist"
process_names = []
pid_allowlist = [4242]

[collection]
focus_signals = [
  "run_queue_delay",
  "offcpu_time",
  "cpu_migration",
  "major_page_fault",
  "subprocess_start_delay",
  "queue_wait",
  "io_latency"
]

[metrics]
track = [
  "ttft",
  "p95_latency",
  "p99_latency",
  "jitter",
  "boost_hit_rate",
  "rollback_count",
  "side_effect_rate"
]
PROFILE

cat >"${PROFILE_SOURCE}/scenarios/tool_call_booster.toml" <<'PROFILE'
[policy]
active_scenarios = ["tool_call_booster"]
evaluation_window_ms = 300
cooldown_ms = 800
max_boost_duration_ms = 800

[triggers.tool_call_booster]
subprocess_start_delay_us = 1500
queue_wait_us = 2000
optional_io_latency_us = 4000

[actions.tool_call_booster]
raise_nice = -3
pin_strategy = "prefer_low_contention_cores"
warmup_executor = true
PROFILE

if [[ -n "${AEGISAI_PACKAGING_DAEMON_BIN:-}" ]]; then
  DAEMON_BIN="${AEGISAI_PACKAGING_DAEMON_BIN}"
elif [[ -n "${AEGISAI_PACKAGING_HELPER_BIN:-}" ]]; then
  echo "AEGISAI_PACKAGING_HELPER_BIN requires AEGISAI_PACKAGING_DAEMON_BIN" >&2
  exit 2
else
  cat >"${DAEMON_BIN}" <<'STUB'
#!/usr/bin/env bash
if [[ "$*" == *"--config-profile production"* ]]; then
  exit 0
fi
echo "expected production profile validation args" >&2
exit 1
STUB
  chmod +x "${DAEMON_BIN}"
fi

if [[ -n "${AEGISAI_PACKAGING_HELPER_BIN:-}" ]]; then
  HELPER_BIN="${AEGISAI_PACKAGING_HELPER_BIN}"
else
  cat >"${HELPER_BIN}" <<'STUB'
#!/usr/bin/env bash
exit 0
STUB
  chmod +x "${HELPER_BIN}"
fi

test -x "${DAEMON_BIN}"
test -x "${HELPER_BIN}"

bash "${SCRIPT_DIR}/install.sh" dry-run \
  --destdir "${DESTDIR}" \
  --profile-source "${PROFILE_SOURCE}" \
  --daemon-bin "${DAEMON_BIN}" \
  --helper-bin "${HELPER_BIN}"

test -x "${DESTDIR}/usr/bin/aegisai-runtime-daemon"
test -x "${DESTDIR}/usr/lib/aegisai/aegisai-ebpf-helper"
test -f "${DESTDIR}/usr/lib/systemd/system/aegisai-runtime.service"
test -f "${DESTDIR}/usr/lib/sysusers.d/aegisai-runtime.conf"
test -f "${DESTDIR}/usr/lib/tmpfiles.d/aegisai-runtime.conf"
test -f "${DESTDIR}/etc/aegisai/configs/profiles/production/runtime.toml"
grep -q '^User=_aegisai$' "${DESTDIR}/usr/lib/systemd/system/aegisai-runtime.service"
grep -q -- '--actuator-backend linux-skeleton' \
  "${DESTDIR}/usr/lib/systemd/system/aegisai-runtime.service"

bash "${SCRIPT_DIR}/install.sh" purge --destdir "${DESTDIR}"
test ! -e "${DESTDIR}/usr/bin/aegisai-runtime-daemon"
test ! -e "${DESTDIR}/etc/aegisai"

printf 'packaging smoke ok\n'
