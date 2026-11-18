#!/usr/bin/env bash
# Privacy guard: refuses to let personal data, media files or secrets into the repo.
#
#   privacy-scan.sh --staged       pre-commit: the index version of every staged file
#   privacy-scan.sh --tracked      pre-push: every tracked file + commit metadata of unpushed commits
#   privacy-scan.sh --ci           like --tracked over all history, without the local denylist
#   privacy-scan.sh --dir PATH     a non-git directory
#
# Output: one line per hit, "privacy-scan: <path>:<line>: <rule>". Exit 1 on any hit.
set -euo pipefail

DENYLIST="$HOME/.config/convodump/privacy-denylist.txt"
MAX_FILE_BYTES=524288
MEDIA_EXTENSIONS="wav mp3 m4a aac flac ogg opus mp4 mov mkv webm avi caf aif aiff amr 3gp wma"
IPV4_PRIVATE='\b10\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\b|\b192\.168\.[0-9]{1,3}\.[0-9]{1,3}\b|\b172\.(1[6-9]|2[0-9]|3[01])\.[0-9]{1,3}\.[0-9]{1,3}\b'
HOME_PATHS='/Users/[A-Za-z]|/home/[a-z_]'
EMAIL='[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
EMAIL_ALLOWED='@(users\.noreply\.github\.com|example\.com)$'
ALLOWED_COMMIT_EMAIL='@users\.noreply\.github\.com$'
GITHUB_WEB_EMAIL='noreply@'"github.com"

MODE="${1:-}"
TARGET="${2:-}"
FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

usage() {
  echo "usage: privacy-scan.sh --staged | --tracked | --ci | --dir PATH" >&2
  exit 2
}

hit() { # path line rule
  printf 'privacy-scan: %s:%s: %s\n' "$1" "$2" "$3"
  FAILED=1
}

report() { # path rule ; stdin = line numbers
  local n
  while IFS= read -r n; do
    [ -n "$n" ] && hit "$1" "$n" "$2"
  done
  return 0
}

USE_DENYLIST=1
case "$MODE" in
  --staged | --tracked) ;;
  --ci) USE_DENYLIST=0 ;;
  --dir) [ -d "$TARGET" ] || usage ;;
  *) usage ;;
esac

if [ "$USE_DENYLIST" = 1 ]; then
  if [ ! -f "$DENYLIST" ]; then
    echo "privacy-scan: denylist missing" >&2
    echo "fix: create ~/.config/convodump/privacy-denylist.txt (see the private deploy notes)" >&2
    exit 1
  fi
  grep -v '^$' "$DENYLIST" > "$WORK/denylist" || true
fi

scan_text() { # label file : generic content rules (a)(b)(c) + denylist (f)
  local label="$1" file="$2"
  report "$label" private-ip < <(grep -naE "$IPV4_PRIVATE" "$file" | cut -d: -f1 | sort -un || true)
  report "$label" home-path < <(grep -naE "$HOME_PATHS" "$file" | cut -d: -f1 | sort -un || true)
  report "$label" email < <(grep -naoE "$EMAIL" "$file" | grep -aviE "$EMAIL_ALLOWED" | cut -d: -f1 | sort -un || true)
  if [ "$USE_DENYLIST" = 1 ]; then
    report "$label" denylist < <(grep -naiF -f "$WORK/denylist" "$file" | cut -d: -f1 | sort -un || true)
  fi
}

scan_file() { # path file
  local path="$1" file="$2" lower ext size
  lower="$(printf '%s' "$path" | tr '[:upper:]' '[:lower:]')"
  for ext in $MEDIA_EXTENSIONS; do
    case "$lower" in *."$ext") hit "$path" 1 media-file ;; esac
  done
  size="$(wc -c < "$file" | tr -d ' ')"
  if [ "$size" -gt "$MAX_FILE_BYTES" ]; then
    hit "$path" 1 file-too-large
  fi
  if [ "$USE_DENYLIST" = 1 ] && printf '%s\n' "$path" | grep -qiF -f "$WORK/denylist"; then
    hit "$path" 1 denylist-in-path
  fi
  scan_text "$path" "$file"
}

scan_paths() { # reads NUL-separated paths on stdin; $1 = how to read: index | disk
  local how="$1" path
  while IFS= read -r -d '' path; do
    if [ "$how" = index ]; then
      git cat-file -p ":$path" > "$WORK/blob"
    else
      cat "$path" > "$WORK/blob"
      path="${path#./}"
    fi
    scan_file "$path" "$WORK/blob"
  done
}

scan_history() {
  local range="$1" email msg
  # (h) author and committer emails
  while IFS= read -r email; do
    [ -n "$email" ] || continue
    if ! printf '%s' "$email" | grep -qE "$ALLOWED_COMMIT_EMAIL" && [ "$email" != "$GITHUB_WEB_EMAIL" ]; then
      hit "commit-metadata" 1 commit-email-not-noreply
    fi
  done < <(git log --format='%ae%n%ce' $range | sort -u)
  # commit messages: generic rules and denylist
  git log --format='%B' $range > "$WORK/messages" || true
  scan_text "commit-message" "$WORK/messages"
}

case "$MODE" in
  --staged)
    scan_paths index < <(git diff --cached --name-only --diff-filter=ACMR -z)
    if ! out="$(gitleaks git --pre-commit --staged --redact --no-banner 2>&1)"; then
      printf '%s\n' "$out" >&2
      hit "(staged)" 1 gitleaks
    fi
    ;;
  --tracked | --ci)
    scan_paths index < <(git ls-files -z)
    if [ "$MODE" = --ci ] || ! git rev-parse --verify -q origin/main > /dev/null; then
      RANGE="HEAD"
    else
      RANGE="origin/main..HEAD"
    fi
    scan_history "$RANGE"
    if ! out="$(gitleaks git --redact --no-banner . 2>&1)"; then
      printf '%s\n' "$out" >&2
      hit "(history)" 1 gitleaks
    fi
    ;;
  --dir)
    cd "$TARGET"
    scan_paths disk < <(find . -type f -not -path './.git/*' -print0)
    if ! out="$(gitleaks dir --redact --no-banner . 2>&1)"; then
      printf '%s\n' "$out" >&2
      hit "$TARGET" 1 gitleaks
    fi
    ;;
esac

if [ "$FAILED" = 1 ]; then
  echo "privacy-scan: FAILED" >&2
  exit 1
fi
echo "privacy-scan: ok" >&2
