#!/usr/bin/env bash
# Cut a new TrUAPI protocol version.
#
# This script performs the release steps for crystallizing the current
# protocol surface:
#
#   1. Moves rust/crates/truapi/src/next/ contents into a new vNN module
#      (only if next/ has types beyond the mod.rs header).
#   2. Creates a fresh empty next/ staging module.
#   3. Takes an explorer version snapshot via snapshot-version.sh.
#   4. Generates CHANGELOG.md from conventional commits and git tags.
#   5. Bumps package.json and Cargo.toml to the next version.
#
# Usage:
#   scripts/cut-version.sh --bump patch   # 0.3.0 -> 0.3.1 (default)
#   scripts/cut-version.sh --bump minor   # 0.3.0 -> 0.4.0
#   scripts/cut-version.sh --bump major   # 0.3.0 -> 1.0.0
#   scripts/cut-version.sh --dry-run      # show what would happen
#
# Prerequisites:
#   - Rust nightly toolchain (for rustdoc JSON)
#
# Commit messages should follow Conventional Commits:
#   feat: ...     -> Added
#   fix: ...      -> Fixed
#   refactor: ... -> Changed
#   docs: ...     -> (skipped)
#   chore: ...    -> (skipped)
#   ci: ...       -> (skipped)
#   BREAKING CHANGE / !: -> Changed (breaking)
#   remove/delete in body -> Removed

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DRY_RUN=0
BUMP="patch"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --bump)
      BUMP="${2:-}"
      case "$BUMP" in
        patch|minor|major) ;;
        *) echo "cut-version: --bump must be patch, minor, or major" >&2; exit 2 ;;
      esac
      shift 2
      ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "cut-version: unknown argument: $1" >&2; exit 2 ;;
  esac
done

VERSION="$(node -e 'console.log(require("./js/packages/truapi/package.json").version)')"
if [ -z "$VERSION" ]; then
  echo "cut-version: could not read version from package.json" >&2
  exit 1
fi

TODAY="$(date +%Y-%m-%d)"
echo "Cutting version $VERSION ($TODAY)"

# --- 1. Crystallize next/ into vNN (if it has content) ---

NEXT_DIR="rust/crates/truapi/src/next"
NEXT_FILES=$(find "$NEXT_DIR" -name '*.rs' ! -name 'mod.rs' 2>/dev/null | wc -l | tr -d ' ')

EXISTING_MAX=0
for d in rust/crates/truapi/src/v[0-9][0-9]; do
  [ -d "$d" ] || continue
  NUM=$(basename "$d" | sed 's/^v0*//')
  [ "${NUM:-0}" -gt "$EXISTING_MAX" ] && EXISTING_MAX="$NUM"
done
NEXT_V_NUM=$((EXISTING_MAX + 1))
NEXT_V_MOD=$(printf 'v%02d' "$NEXT_V_NUM")
NEXT_V_DIR="rust/crates/truapi/src/$NEXT_V_MOD"

if [ "$NEXT_FILES" -gt 0 ]; then
  echo "  Crystallizing next/ -> $NEXT_V_MOD/ ($NEXT_FILES type files)"
  if [ "$DRY_RUN" -eq 0 ]; then
    cp -r "$NEXT_DIR" "$NEXT_V_DIR"
    cat > "$NEXT_V_DIR/mod.rs" <<MODEOF
//! TrUAPI Protocol $NEXT_V_MOD type definitions.
MODEOF
    awk 'NR > 1 && !/^\/\/!/' "$NEXT_DIR/mod.rs" >> "$NEXT_V_DIR/mod.rs" || true
  fi
else
  echo "  next/ has no type files to crystallize (v01 types unchanged for $VERSION)"
fi

# --- 2. Reset next/ ---

echo "  Resetting next/ staging module"
if [ "$DRY_RUN" -eq 0 ]; then
  rm -rf "$NEXT_DIR"
  mkdir -p "$NEXT_DIR"
  cat > "$NEXT_DIR/mod.rs" <<'NEXTEOF'
//! Staging area for wire types targeting the next protocol release.
//!
//! Types here are under active development and not yet crystallized.
//! When a version is cut (`scripts/cut-version.sh`), this module's
//! contents are moved into a new `vNN` module and a fresh empty
//! `next` is created.
NEXTEOF
fi

# --- 3. Explorer snapshot ---

echo "  Taking explorer snapshot for $VERSION"
if [ "$DRY_RUN" -eq 0 ]; then
  bash "$ROOT/scripts/snapshot-version.sh" --force
fi

# --- 4. Generate CHANGELOG.md from conventional commits ---

echo "  Generating CHANGELOG.md"

# Collect version tags sorted by semver (oldest first).
# Expected tag format: v0.1.0 or @parity/truapi@0.1.0
read_tags() {
  git tag -l 'v*' --sort=version:refname 2>/dev/null
  git tag -l '@parity/truapi@*' --sort=version:refname 2>/dev/null
}

# Extract the semver portion from a tag name.
tag_version() {
  echo "$1" | sed 's/^.*@//; s/^v//'
}

# Classify a conventional commit subject into a changelog section.
# Returns: Added|Fixed|Changed|Removed|"" (skip)
classify_commit() {
  local subject="$1"
  # Skip merge commits
  case "$subject" in Merge\ *) return ;; esac

  # Breaking change
  if echo "$subject" | grep -qE '^[a-z]+(\(.+\))?!:'; then
    echo "Changed"; return
  fi

  local type
  type=$(echo "$subject" | sed -nE 's/^([a-z]+)(\(.+\))?: .*/\1/p')
  case "$type" in
    feat)     echo "Added" ;;
    fix)      echo "Fixed" ;;
    refactor) echo "Changed" ;;
    perf)     echo "Changed" ;;
    remove)   echo "Removed" ;;
    revert)   echo "Removed" ;;
    # docs, chore, ci, test, style, build -> skip
    *)
      # Non-conventional commits: include as Changed if not empty
      if [ -n "$subject" ] && ! echo "$subject" | grep -qE '^(docs|chore|ci|test|style|build)(\(.+\))?:'; then
        echo "Changed"
      fi
      ;;
  esac
}

# Strip the conventional prefix from a subject line for display.
format_subject() {
  local subject="$1"
  # Remove type(scope)!: or type: prefix
  echo "$subject" | sed -E 's/^[a-z]+(\([^)]+\))?!?: //'
}

## List RFC .md files at a given git ref (excludes _index.md).
# Returns lines like "0017-coin-payment.md"
rfcs_at_ref() {
  git ls-tree --name-only "$1" -- docs/rfcs/ 2>/dev/null \
    | sed 's|^docs/rfcs/||' \
    | grep -E '^[0-9]{4}-.*\.md$' || true
}

# Extract the RFC title from a file at a given ref.
# Tries YAML frontmatter `title:`, then first H1 heading.
rfc_title_at_ref() {
  local ref="$1" file="$2"
  local title
  title=$(git show "${ref}:docs/rfcs/${file}" 2>/dev/null \
    | awk '/^---$/{n++; next} n==1 && /^title:/{sub(/^title: *"?/, ""); sub(/"? *$/, ""); print; exit}')
  if [ -z "$title" ]; then
    title=$(git show "${ref}:docs/rfcs/${file}" 2>/dev/null \
      | grep -m1 '^# ' | sed 's/^# *//')
  fi
  echo "${title:-$file}"
}

# Diff RFCs between two refs. Outputs added/removed lines.
# Args: old_ref new_ref
# Output: "+0017-coin-payment.md" or "-0010-get-root-account.md"
diff_rfcs() {
  local old_ref="$1" new_ref="$2"
  local old_rfcs new_rfcs
  old_rfcs=$(rfcs_at_ref "$old_ref")
  new_rfcs=$(rfcs_at_ref "$new_ref")
  diff <(echo "$old_rfcs" | sort) <(echo "$new_rfcs" | sort) \
    | grep '^[<>]' | sed 's/^< /-/; s/^> /+/' || true
}

generate_changelog() {
  local changelog="$1"

  {
    echo "# Changelog"
    echo ""
    echo "All notable changes to the TrUAPI protocol are documented in this file."
    echo ""
    echo "The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),"
    echo "generated from [Conventional Commits](https://www.conventionalcommits.org/)."
    echo ""

    # Build ordered list of ranges: tag pairs + HEAD
    local tags=()
    local versions=()
    while IFS= read -r tag; do
      [ -z "$tag" ] && continue
      tags+=("$tag")
      versions+=("$(tag_version "$tag")")
    done < <(read_tags)

    # Current release (tag for VERSION may not exist yet)
    local ranges=()
    local labels=()
    local dates=()
    local old_refs=()
    local new_refs=()

    if [ "${#tags[@]}" -gt 0 ]; then
      local latest_tag="${tags[${#tags[@]}-1]}"
      ranges+=("${latest_tag}..HEAD")
      labels+=("$VERSION")
      dates+=("$TODAY")
      old_refs+=("$latest_tag")
      new_refs+=("HEAD")

      for ((i=${#tags[@]}-1; i>0; i--)); do
        ranges+=("${tags[i-1]}..${tags[i]}")
        labels+=("${versions[i]}")
        dates+=("$(git log -1 --format=%as "${tags[i]}")")
        old_refs+=("${tags[i-1]}")
        new_refs+=("${tags[i]}")
      done

      # First tag: everything up to it
      ranges+=("${tags[0]}")
      labels+=("${versions[0]}")
      dates+=("$(git log -1 --format=%as "${tags[0]}")")
      old_refs+=("")
      new_refs+=("${tags[0]}")
    else
      ranges+=("HEAD")
      labels+=("$VERSION")
      dates+=("$TODAY")
      old_refs+=("")
      new_refs+=("HEAD")
    fi

    for ((r=0; r<${#ranges[@]}; r++)); do
      local range="${ranges[r]}"
      local label="${labels[r]}"
      local date="${dates[r]}"
      local old_ref="${old_refs[r]}"
      local new_ref="${new_refs[r]}"

      local added=() fixed=() changed=() removed=()

      while IFS= read -r subject; do
        [ -z "$subject" ] && continue
        local section
        section=$(classify_commit "$subject")
        [ -z "$section" ] && continue
        local msg
        msg=$(format_subject "$subject")
        case "$section" in
          Added)   added+=("$msg") ;;
          Fixed)   fixed+=("$msg") ;;
          Changed) changed+=("$msg") ;;
          Removed) removed+=("$msg") ;;
        esac
      done < <(git log --format="%s" "$range" -- 2>/dev/null || git log --format="%s" "$range")

      # Collect RFC changes
      local rfcs_added=() rfcs_removed=()
      if [ -n "$old_ref" ]; then
        while IFS= read -r line; do
          [ -z "$line" ] && continue
          local file="${line:1}"
          local title
          case "$line" in
            +*)
              title=$(rfc_title_at_ref "$new_ref" "$file")
              rfcs_added+=("$title")
              ;;
            -*)
              title=$(rfc_title_at_ref "$old_ref" "$file")
              rfcs_removed+=("$title")
              ;;
          esac
        done < <(diff_rfcs "$old_ref" "$new_ref")
      else
        # First version: all RFCs are new
        while IFS= read -r file; do
          [ -z "$file" ] && continue
          local title
          title=$(rfc_title_at_ref "$new_ref" "$file")
          rfcs_added+=("$title")
        done < <(rfcs_at_ref "$new_ref")
      fi

      # Skip empty versions
      local total=$(( ${#added[@]} + ${#fixed[@]} + ${#changed[@]} + ${#removed[@]} + ${#rfcs_added[@]} + ${#rfcs_removed[@]} ))
      if [ "$total" -eq 0 ]; then continue; fi

      echo "## [$label] - $date"
      echo ""

      # RFCs section
      if [ "${#rfcs_added[@]}" -gt 0 ] || [ "${#rfcs_removed[@]}" -gt 0 ]; then
        echo "### RFCs"
        echo ""
        for item in "${rfcs_added[@]+"${rfcs_added[@]}"}"; do
          [ -n "$item" ] && echo "- **Accepted:** $item"
        done
        for item in "${rfcs_removed[@]+"${rfcs_removed[@]}"}"; do
          [ -n "$item" ] && echo "- **Withdrawn:** $item"
        done
        echo ""
      fi

      for section_name in Added Changed Fixed Removed; do
        local items=()
        case "$section_name" in
          Added)   items=("${added[@]+"${added[@]}"}") ;;
          Changed) items=("${changed[@]+"${changed[@]}"}") ;;
          Fixed)   items=("${fixed[@]+"${fixed[@]}"}") ;;
          Removed) items=("${removed[@]+"${removed[@]}"}") ;;
        esac
        if [ "${#items[@]}" -gt 0 ] && [ -n "${items[0]:-}" ]; then
          echo "### $section_name"
          echo ""
          for item in "${items[@]}"; do
            echo "- $item"
          done
          echo ""
        fi
      done
    done

  } > "$changelog"
}

if [ "$DRY_RUN" -eq 0 ]; then
  generate_changelog "$ROOT/CHANGELOG.md"
  echo "  Wrote CHANGELOG.md"
else
  echo "  (dry-run) Would generate CHANGELOG.md"
fi

# --- 5. Bump version ---

IFS='.' read -r V_MAJOR V_MINOR V_PATCH <<< "$VERSION"
case "$BUMP" in
  patch) NEXT_VERSION="${V_MAJOR}.${V_MINOR}.$((V_PATCH + 1))" ;;
  minor) NEXT_VERSION="${V_MAJOR}.$((V_MINOR + 1)).0" ;;
  major) NEXT_VERSION="$((V_MAJOR + 1)).0.0" ;;
esac

echo "  Bumping version: $VERSION -> $NEXT_VERSION"
if [ "$DRY_RUN" -eq 0 ]; then
  node -e "
    const fs = require('fs');
    const path = './js/packages/truapi/package.json';
    const pkg = JSON.parse(fs.readFileSync(path, 'utf8'));
    pkg.version = '${NEXT_VERSION}';
    fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + '\n');
  "

  perl -pi -e "s/^version = \"${VERSION}\"/version = \"${NEXT_VERSION}\"/" \
    rust/crates/truapi/Cargo.toml
fi

echo "Done. Review changes, commit, and tag v${VERSION}. Working version is now ${NEXT_VERSION}."
