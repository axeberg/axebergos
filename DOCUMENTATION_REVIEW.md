# AxebergOS Documentation Review Report

**Date:** 2025-12-25
**Scope:** All documentation files, man pages, comments, and scdoc files
**Files Reviewed:** 26 Markdown files + 87 man pages + TLA+ specs

---

## Executive Summary

A comprehensive review of all AxebergOS documentation revealed **70+ issues** across various categories. The most significant findings are:

1. **CODE_REVIEW.md is significantly outdated** - 5 of 6 critical issues have been fixed but aren't reflected
2. **compositor.md describes a non-existent component** - entire document is invalid
3. **Test counts are severely wrong** - docs claim 99-380 tests, actual count is 674
4. **Multiple kernel docs have incorrect struct definitions** - executor.md, overview.md need major updates
5. **30+ man pages have issues** ranging from undocumented options to documented-but-unimplemented features

---

## Issues by Severity

### CRITICAL (Immediate Action Required)

| File | Line | Issue |
|------|------|-------|
| `docs/userspace/compositor.md` | All | Entire document describes non-existent component - DELETE or MOVE to plans/ |
| `CODE_REVIEW.md` | 22-161 | 5 major issues marked as problems are actually FIXED in code |
| `docs/development/testing.md` | 7 | Claims "99 tests" - actual: 674 tests |
| `docs/kernel/executor.md` | 15-48 | Executor and Task struct definitions are completely wrong |
| `docs/kernel/overview.md` | 76-87 | Executor struct and Priority enum definitions incorrect |
| `README.md` | 5 | Typo: "Anthorpic" → "Anthropic" |
| `man/grep.1.scd` | 18-25 | Documents `-i`, `-n`, `-v` options that are NOT implemented |
| `man/ls.1.scd` | 9-22 | Documents `-l`, `-a` options that are NOT implemented |
| `man/passwd.1.scd` | 18-19 | Claims passwords aren't stored - FALSE, they ARE stored |
| `man/mkfifo.1.scd` | 11, 23 | Duplicate `# DESCRIPTION` header (scdoc syntax error) |

### HIGH PRIORITY

| File | Line | Issue |
|------|------|-------|
| `docs/development/building.md` | 26-57 | Project structure shows non-existent `compositor/`, `runtime.rs`, `static/` |
| `docs/kernel/processes.md` | 48-51 | Missing `ProcessState::Stopped` variant |
| `docs/kernel/signals.md` | 47-55 | Missing `SignalAction::Kill` variant |
| `docs/userspace/vfs.md` | 115-131 | Wrong type name (`OpenFlags` vs `OpenOptions`), incomplete Metadata struct |
| `docs/userspace/shell.md` | 280-289 | Parser structures incorrect (SimpleCommand, Redirect) |
| `docs/architecture/bare-metal.md` | 145-161 | Platform trait doesn't match actual implementation |
| `man/df.1.scd` | 18-22 | Duplicate `-h` option (human-readable AND help) |
| `man/du.1.scd` | 17-24 | Same duplicate `-h` issue as df |
| `man/fsreset.1.scd` | 9 | Synopsis shows `-f` as optional but it's required |
| `man/sort.1.scd` | 20-21 | Documents `-n` numeric sort that isn't implemented |
| `man/strace.1.scd` | 24 | Documents `-e` filter that isn't implemented |
| `man/tail.1.scd` | 78-79 | Documents multiple files but only stdin is supported |
| `specs/tla/README.md` | All | Missing docs for PathValidation.tla, HistoryBuffer.tla |

### MEDIUM PRIORITY

| File | Line | Issue |
|------|------|-------|
| `README.md` | 155 | Project structure lists non-existent `compositor/` directory |
| `README.md` | 58 | Architecture diagram shows "Compositor" but should be "Terminal (xterm.js)" |
| `docs/index.md` | 160 | Claims "380+ tests" - should be "674 tests" |
| `docs/development/building.md` | 7 | Rust version "1.70+" may be too low for edition 2024 |
| `docs/kernel/syscalls.md` | 18-26 | OpenFlags shown as constants but are struct fields |
| `docs/userspace/stdio.md` | 32-37 | ConsoleObject field names wrong (`input_buffer` → `input`) |
| `man/bg.1.scd` | 18-24 | Documents `%string` job spec format not implemented |
| `man/fg.1.scd` | 18-24 | Same `%string` issue |
| `man/chmod.1.scd` | 17 | Says "can be specified in octal" implying others work - only octal works |
| `man/date.1.scd` | 9 | Synopsis shows `+FORMAT` but format is ignored |
| `man/xargs.1.scd` | 44-46 | Limitation (doesn't execute) should be in DESCRIPTION |
| `man/proc.5.scd` | 40 | Missing `/proc/mounts` documentation |
| `man/proc.5.scd` | 71 | Missing `/proc/[pid]/exe` documentation |
| `man/proc.5.scd` | 64 | Documents `/proc/[pid]/statm` that doesn't exist |
| `specs/tla/README.md` | 35 | S3 signal coalescing missing "except SIGKILL" |

### LOW PRIORITY (Minor Issues)

| File | Line | Issue |
|------|------|-------|
| `Cargo.toml` | 4 | Edition "2024" may cause compatibility issues |
| `man/autosave.1.scd` | 72 | References `save(1)` - verify it exists |
| `man/cal.1.scd` | 47-49 | Example "cal 2024" explanation is misleading |
| `man/echo.1.scd` | 21-22 | Missing `\r`, `\a`, `\b` escape documentation |
| `man/expr.1.scd` | 29, 84-87 | Escaping issues in examples |
| `man/tr.1.scd` | 32-35 | Example output incorrect |
| `man/useradd.1.scd` | 1 | Section 8 but filename is `.1.scd` |
| `man/touch.1.scd` | - | Missing OPTIONS section for --help |
| `man/tree.1.scd` | - | Missing OPTIONS section for --help |
| `man/cat.1.scd` | - | Missing OPTIONS section for --help |
| `man/cp.1.scd` | - | Missing OPTIONS section for --help |
| `man/diff.1.scd` | - | Missing OPTIONS section for --help |
| `man/devfs.5.scd` | 66 | References non-existent `null(4)` |
| `man/mount.8.scd` | 65, 69 | References non-existent `/etc/fstab` and `fstab(5)` |

---

## Files Without Issues (Verified Accurate)

### Kernel Documentation
- `docs/kernel/tracing.md` ✓
- `docs/kernel/users.md` ✓
- `docs/kernel/objects.md` ✓
- `docs/kernel/ipc.md` ✓
- `docs/kernel/timers.md` ✓
- `docs/kernel/memory.md` ✓

### Man Pages (Section 1)
- `basename.1.scd`, `chgrp.1.scd`, `comm.1.scd`, `cut.1.scd`, `dirname.1.scd`
- `edit.1.scd`, `find.1.scd`, `findmnt.1.scd`, `fold.1.scd`, `free.1.scd`, `fsload.1.scd`
- `groupadd.1.scd`, `groups.1.scd`, `head.1.scd`, `hostname.1.scd`, `id.1.scd`
- `ipcrm.1.scd`, `ipcs.1.scd`, `jobs.1.scd`, `kill.1.scd`, `ln.1.scd`, `man.1.scd`
- `mkdir.1.scd`, `mv.1.scd`, `nl.1.scd`, `paste.1.scd`, `printenv.1.scd`, `printf.1.scd`
- `ps.1.scd`, `pwd.1.scd`, `rev.1.scd`, `rm.1.scd`, `save.1.scd`, `seq.1.scd`
- `strings.1.scd`, `su.1.scd`, `tee.1.scd`, `test.1.scd`, `time.1.scd`, `tty.1.scd`
- `type.1.scd`, `uname.1.scd`, `uniq.1.scd`, `uptime.1.scd`, `wc.1.scd`, `which.1.scd`
- `whoami.1.scd`, `xxd.1.scd`, `yes.1.scd`

### Man Pages (Other Sections)
- `sysfs.5.scd` ✓
- `intro.7.scd` ✓
- `umount.8.scd` ✓

---

## Recommended Action Plan

### Phase 1: Critical Fixes (Do First)

1. **Delete or relocate** `docs/userspace/compositor.md`
2. **Update** `CODE_REVIEW.md` to mark fixed issues as resolved
3. **Fix** test counts in `testing.md` (99 → 674) and `index.md` (380 → 674)
4. **Correct** `README.md` typo and project structure
5. **Fix** `executor.md` and `overview.md` struct definitions
6. **Fix** man page scdoc syntax error in `mkfifo.1.scd`

### Phase 2: High Priority Fixes

1. **Remove** undocumented options from `grep.1.scd`, `ls.1.scd` (or implement them)
2. **Correct** `passwd.1.scd` misinformation about password storage
3. **Fix** duplicate `-h` in `df.1.scd`, `du.1.scd`
4. **Update** `building.md` project structure
5. **Document** missing TLA+ specs in `specs/tla/README.md`

### Phase 3: Medium Priority

1. Update all struct/enum definitions in kernel docs
2. Fix VFS and shell documentation inaccuracies
3. Add missing `/proc/mounts` and `/proc/[pid]/exe` to proc.5.scd
4. Fix job spec documentation in bg/fg man pages
5. Clarify architecture docs as "future work"

### Phase 4: Polish

1. Add missing OPTIONS sections to man pages
2. Fix minor example errors
3. Correct file naming (useradd.1.scd → useradd.8.scd)
4. Remove broken cross-references

---

## Statistics

| Category | Files | Issues Found | Clean |
|----------|-------|--------------|-------|
| Root docs (README, CODE_REVIEW) | 2 | 8 | 0 |
| Kernel docs | 12 | 17 | 6 |
| Userspace docs | 4 | 9 | 0 |
| Development docs | 5 | 12 | 1 |
| Architecture/Plans docs | 2 | 11 | 0 |
| Man pages (section 1) | 81 | 30 | 51 |
| Man pages (sections 5,7,8) | 6 | 4 | 3 |
| TLA+ specs | 6 | 5 | 3 |
| **TOTAL** | **118** | **96** | **64** |

---

## Conclusion

The AxebergOS documentation is extensive but contains significant accuracy issues. The codebase has evolved considerably (security fixes, refactoring, new features) but documentation hasn't kept pace. The most critical action is updating `CODE_REVIEW.md` to reflect the current (improved) state of the code, and removing or relocating the compositor documentation that describes a non-existent component.

**Documentation Quality Score: 6/10**
- Strong: Man pages are mostly accurate, kernel IPC/timers/users/objects docs are solid
- Weak: Outdated code review, wrong test counts, non-existent compositor docs, incorrect struct definitions
