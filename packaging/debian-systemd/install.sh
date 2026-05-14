#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." >/dev/null 2>&1 && pwd)"

PACKAGE_NAME="aegisai-runtime"
SERVICE_NAME="aegisai-runtime.service"
SERVICE_USER="_aegisai"
SERVICE_GROUP="_aegisai"

DESTDIR=""
PROFILE_SOURCE=""
DAEMON_BIN="${REPO_ROOT}/target/release/aegisai-runtime-daemon"
HELPER_BIN="${REPO_ROOT}/target/release/aegisai-ebpf-helper"

usage() {
  cat <<'USAGE'
Usage: packaging/debian-systemd/install.sh <command> [options]

Commands:
  preflight   Validate host/package prerequisites without installing files.
  install     Install files, reload systemd, and enable the service.
  dry-run     Stage files under --destdir and validate the package layout.
  remove      Stop/disable service and remove package-owned units and binaries.
  purge       Run remove, then remove package-owned config, state, and logs.

Options:
  --destdir <path>          Stage under a destination root instead of the host.
  --profile-source <path>   Production profile directory to install.
  --daemon-bin <path>       Built aegisai-runtime-daemon binary.
  --helper-bin <path>       Built aegisai-ebpf-helper binary.
  -h, --help                Show this help.

The production profile source must contain non-example files matching
configs/profiles/production/ from docs/packaging_contract.md.
USAGE
}

log() {
  printf '%s\n' "$*"
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

dest_path() {
  local absolute="$1"
  if [[ -n "${DESTDIR}" ]]; then
    printf '%s%s\n' "${DESTDIR%/}" "${absolute}"
  else
    printf '%s\n' "${absolute}"
  fi
}

need_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

kernel_ge_5_15() {
  local release major minor
  release="$(uname -r)"
  major="${release%%.*}"
  minor="${release#*.}"
  minor="${minor%%.*}"
  [[ "${major}" =~ ^[0-9]+$ && "${minor}" =~ ^[0-9]+$ ]] ||
    die "could not parse kernel release: ${release}"
  ((major > 5 || (major == 5 && minor >= 15)))
}

require_linux_host() {
  [[ "$(uname -s)" == "Linux" ]] || die "unsupported OS: $(uname -s); Linux is required"
}

require_systemd_host() {
  [[ -d /run/systemd/system ]] || die "system systemd is not active at /run/systemd/system"
  need_command systemctl
}

require_cgroup_v2() {
  [[ -f /sys/fs/cgroup/cgroup.controllers ]] ||
    die "cgroup v2 is required at /sys/fs/cgroup/cgroup.controllers"
}

require_root_for_host_install() {
  if [[ -z "${DESTDIR}" && "${EUID}" -ne 0 ]]; then
    die "host install/remove/purge requires root; use --destdir for staging"
  fi
}

require_binaries() {
  [[ -x "${DAEMON_BIN}" ]] ||
    die "missing executable daemon binary: ${DAEMON_BIN}; run cargo build --release -p aegisai-runtime-daemon"
  [[ -x "${HELPER_BIN}" ]] ||
    die "missing executable helper binary: ${HELPER_BIN}; run cargo build --release -p aegisai-ebpf-helper"
}

default_profile_source() {
  local candidate="${REPO_ROOT}/configs/profiles/production"
  if [[ -d "${candidate}" ]]; then
    PROFILE_SOURCE="${candidate}"
  fi
}

require_profile_source() {
  if [[ -z "${PROFILE_SOURCE}" ]]; then
    default_profile_source
  fi
  [[ -n "${PROFILE_SOURCE}" ]] ||
    die "missing production profile source; pass --profile-source <dir>"
  [[ -d "${PROFILE_SOURCE}" ]] || die "profile source is not a directory: ${PROFILE_SOURCE}"
  local required=(
    "runtime.toml"
    "classifier/process_rules.toml"
    "scenarios/ai_workload_awareness.toml"
    "scenarios/inference_tail_guard.toml"
    "scenarios/tool_call_booster.toml"
    "safety/default.toml"
  )
  local relative
  for relative in "${required[@]}"; do
    [[ -f "${PROFILE_SOURCE}/${relative}" ]] ||
      die "profile source missing required file: ${relative}"
    [[ "${relative}" != *".example.toml" ]] ||
      die "profile source must contain non-example file names"
  done
}

validate_profile_with_daemon() {
  if [[ -x "${DAEMON_BIN}" ]]; then
    local temp_root
    temp_root="$(mktemp -d)"
    trap 'rm -rf "${temp_root}"' RETURN
    mkdir -p "${temp_root}/configs/profiles"
    cp -a "${PROFILE_SOURCE}" "${temp_root}/configs/profiles/production"
    "${DAEMON_BIN}" \
      --repo-root "${temp_root}" \
      --config-profile production \
      --max-events 1 \
      >/dev/null
    rm -rf "${temp_root}"
    trap - RETURN
  fi
}

preflight() {
  require_linux_host
  kernel_ge_5_15 || die "kernel $(uname -r) is below required 5.15"
  if [[ -z "${DESTDIR}" ]]; then
    require_systemd_host
    require_cgroup_v2
    require_root_for_host_install
  fi
  require_binaries
  require_profile_source
  validate_profile_with_daemon
  log "preflight ok"
}

install_file() {
  local source="$1"
  local target="$2"
  local mode="$3"
  install -D -m "${mode}" "${source}" "$(dest_path "${target}")"
}

install_profile() {
  local target
  target="$(dest_path /etc/aegisai/configs/profiles/production)"
  rm -rf "${target}"
  mkdir -p "${target}"
  cp -a "${PROFILE_SOURCE}/." "${target}/"
}

install_layout() {
  install_file "${DAEMON_BIN}" /usr/bin/aegisai-runtime-daemon 0755
  install_file "${HELPER_BIN}" /usr/lib/aegisai/aegisai-ebpf-helper 0755
  install_file "${SCRIPT_DIR}/${SERVICE_NAME}" "/usr/lib/systemd/system/${SERVICE_NAME}" 0644
  install_file "${SCRIPT_DIR}/${PACKAGE_NAME}.sysusers" "/usr/lib/sysusers.d/${PACKAGE_NAME}.conf" 0644
  install_file "${SCRIPT_DIR}/${PACKAGE_NAME}.tmpfiles" "/usr/lib/tmpfiles.d/${PACKAGE_NAME}.conf" 0644
  install_file "${REPO_ROOT}/docs/packaging_contract.md" "/usr/share/doc/${PACKAGE_NAME}/packaging_contract.md" 0644
  install_profile
  install -d -m 0750 "$(dest_path /var/lib/aegisai)" "$(dest_path /var/log/aegisai)" "$(dest_path /run/aegisai)"
}

apply_host_ownership() {
  chown -R root:root /usr/lib/aegisai /usr/share/doc/${PACKAGE_NAME}
  chown root:root /usr/bin/aegisai-runtime-daemon
  chown -R "${SERVICE_USER}:${SERVICE_GROUP}" /var/lib/aegisai /var/log/aegisai /run/aegisai
}

activate_service() {
  systemd-sysusers "/usr/lib/sysusers.d/${PACKAGE_NAME}.conf"
  systemd-tmpfiles --create "/usr/lib/tmpfiles.d/${PACKAGE_NAME}.conf"
  systemctl daemon-reload
  systemctl enable --now "${SERVICE_NAME}"
}

install_package() {
  preflight
  install_layout
  if [[ -n "${DESTDIR}" ]]; then
    log "staged ${PACKAGE_NAME} under ${DESTDIR}"
  else
    systemd-sysusers "/usr/lib/sysusers.d/${PACKAGE_NAME}.conf"
    apply_host_ownership
    activate_service
    log "installed and started ${SERVICE_NAME}"
  fi
}

remove_package() {
  require_root_for_host_install
  if [[ -z "${DESTDIR}" ]]; then
    if command -v systemctl >/dev/null 2>&1; then
      systemctl disable --now "${SERVICE_NAME}" >/dev/null 2>&1 || true
      systemctl daemon-reload || true
    fi
  fi
  rm -f \
    "$(dest_path /usr/bin/aegisai-runtime-daemon)" \
    "$(dest_path /usr/lib/aegisai/aegisai-ebpf-helper)" \
    "$(dest_path "/usr/lib/systemd/system/${SERVICE_NAME}")" \
    "$(dest_path "/usr/lib/sysusers.d/${PACKAGE_NAME}.conf")" \
    "$(dest_path "/usr/lib/tmpfiles.d/${PACKAGE_NAME}.conf")"
  rmdir "$(dest_path /usr/lib/aegisai)" 2>/dev/null || true
  rm -rf "$(dest_path "/usr/share/doc/${PACKAGE_NAME}")"
  log "removed package-owned service and binary files"
}

purge_package() {
  remove_package
  rm -rf \
    "$(dest_path /etc/aegisai)" \
    "$(dest_path /var/lib/aegisai)" \
    "$(dest_path /var/log/aegisai)" \
    "$(dest_path /run/aegisai)"
  log "purged package-owned config, state, and logs"
}

dry_run() {
  [[ -n "${DESTDIR}" ]] || die "dry-run requires --destdir <path>"
  install_package
  [[ -x "$(dest_path /usr/bin/aegisai-runtime-daemon)" ]] || die "daemon was not staged"
  [[ -x "$(dest_path /usr/lib/aegisai/aegisai-ebpf-helper)" ]] || die "helper was not staged"
  [[ -f "$(dest_path "/usr/lib/systemd/system/${SERVICE_NAME}")" ]] || die "service was not staged"
  [[ -f "$(dest_path /etc/aegisai/configs/profiles/production/runtime.toml)" ]] ||
    die "production profile was not staged"
  log "dry-run ok"
}

COMMAND="${1:-}"
if [[ -z "${COMMAND}" ]]; then
  usage >&2
  exit 2
fi
shift || true

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --destdir)
      DESTDIR="${2:-}"
      [[ -n "${DESTDIR}" ]] || die "--destdir expects a path"
      shift 2
      ;;
    --profile-source)
      PROFILE_SOURCE="${2:-}"
      [[ -n "${PROFILE_SOURCE}" ]] || die "--profile-source expects a path"
      shift 2
      ;;
    --daemon-bin)
      DAEMON_BIN="${2:-}"
      [[ -n "${DAEMON_BIN}" ]] || die "--daemon-bin expects a path"
      shift 2
      ;;
    --helper-bin)
      HELPER_BIN="${2:-}"
      [[ -n "${HELPER_BIN}" ]] || die "--helper-bin expects a path"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
done

case "${COMMAND}" in
  preflight)
    preflight
    ;;
  install)
    install_package
    ;;
  dry-run)
    dry_run
    ;;
  remove)
    remove_package
    ;;
  purge)
    purge_package
    ;;
  -h | --help)
    usage
    ;;
  *)
    die "unknown command: ${COMMAND}"
    ;;
esac
