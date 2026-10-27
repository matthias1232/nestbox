"""Fixed inventory script, executed inside a sandbox container.

Nothing in here takes a parameter. The runner reads this file and pipes it to
``sh`` inside the container, so what it prints is the same on every sandbox and
an agent inside the container cannot influence it.

Format: sections introduced by ``[name]``, one record per line, fields separated
by a tab. The parser in ``app/inventory.py`` turns that into JSON. A section that
cannot be read prints nothing, which the parser reports as unknown rather than
empty-and-fine.

Two rules that are not negotiable:

* environment variables are printed by **name only** (``env`` is cut at the
  first ``=``), so a token value cannot reach the control plane through an
  inventory;
* files are listed by name, never read, for anything under the agent's home.
"""

set -u

section() {
    printf '\n[%s]\n' "$1"
}

# ---------------------------------------------------------------- operating system
section os
while IFS= read -r line; do
    case "$line" in
        NAME=*|VERSION=*|VERSION_ID=*|ID=*|PRETTY_NAME=*|VERSION_CODENAME=*) printf '%s\n' "$line" ;;
    esac
done < /etc/os-release 2>/dev/null

section kernel
uname -sr 2>/dev/null
printf 'arch\t%s\n' "$(uname -m 2>/dev/null)"

# --------------------------------------------------------------------- packages
section packages
if command -v dpkg-query >/dev/null 2>&1; then
    dpkg-query -W -f='${Package}\t${Version}\n' 2>/dev/null
elif command -v rpm >/dev/null 2>&1; then
    rpm -qa --qf '%{NAME}\t%{VERSION}-%{RELEASE}\n' 2>/dev/null
elif command -v apk >/dev/null 2>&1; then
    apk info -v 2>/dev/null | sed 's/-\([0-9][^-]*\)$/\t\1/'
fi

# ----------------------------------------------------------------------- agents
# name<TAB>version, or name<TAB> with an empty version when the tool is absent.
section agents
for tool in hermes openclaw openchamber opencode claude codex node npm python3 git ollama; do
    if command -v "$tool" >/dev/null 2>&1; then
        version=""
        case "$tool" in
            node) version="$(node -v 2>/dev/null)" ;;
            python3) version="$(python3 -V 2>&1)" ;;
            *) version="$("$tool" --version 2>/dev/null | head -n 1)" ;;
        esac
        printf '%s\t%s\n' "$tool" "$version"
    fi
done

# --------------------------------------------------------------- listening ports
section listening
if command -v ss >/dev/null 2>&1; then
    ss -ltnH 2>/dev/null | awk '{print $4"\t"$1}' | sort -u
elif command -v netstat >/dev/null 2>&1; then
    netstat -ltn 2>/dev/null | awk 'NR>2 {print $4"\t"$1}' | sort -u
fi

# ------------------------------------------------------------ environment (names)
section env_keys
env 2>/dev/null | cut -d= -f1 | sort -u

# ----------------------------------------------------------------------- mounts
section mounts
awk '$2 ~ /^\// {print $2"\t"$3"\t"$1}' /proc/mounts 2>/dev/null | grep -v -E '^\S+\s(cgroup|proc|sysfs|devpts|mqueue|tmpfs|shm)\s' | head -n 40

# ------------------------------------------------------------------------ MCP config
# Names only: `hermes mcp list` prints server names and transports, not tokens.
section mcp_servers
for config in /home/nestbox/.hermes/config.yaml /home/nestbox/.config/openchamber/mcp.json; do
    if [ -r "$config" ]; then
        awk '/^[[:space:]]{0,4}[a-zA-Z0-9_.-]+:$/ {gsub(/[[:space:]:]/, ""); print}' "$config" 2>/dev/null | head -n 40
    fi
done

printf '\n[done]\n'
