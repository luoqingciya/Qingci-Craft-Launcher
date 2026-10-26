# Additional Terms (GNU GPLv3, Section 7)

[中文](ADDITIONAL-TERMS.md) | **English**

> **This is a translation provided for convenience.** The Chinese version
> ([ADDITIONAL-TERMS.md](ADDITIONAL-TERMS.md)) is authoritative; if the two differ, the Chinese
> version governs.

The copyright holder of this project supplements the terms of the GNU General Public License,
version 3 (GPLv3), with the following additional terms, pursuant to **Section 7** of that license.
They apply to the work created by the copyright holder in this repository (the "Work").

These additional terms and the GPLv3 text in [`LICENSE`](LICENSE) together form the complete
license conditions. **The GPLv3 text itself is unmodified** — Section 7 permits supplementing it
without editing the license document.

---

## 1. Preserving copyright and license notices

> Pursuant to GPLv3 §7(b): you may require preservation of specified reasonable legal notices or
> author attributions.

When distributing the Work or a modified version, you must preserve in full:

- the original copyright notice, e.g. `Copyright (C) 2026 luoqingci` in source file headers
- the license identifier `SPDX-License-Identifier: GPL-3.0-only`
- the notice pointing to this document, or an explicit pointer to where it can be obtained

You may not remove, obscure, rewrite or hide them. If you distribute binaries, the notices must
appear in the program's own "About" screen, accompanying documentation, or another
user-visible location — not only in source code.

## 2. Modified versions must be renamed

> Pursuant to GPLv3 §7(c): you may require that modified versions be marked in reasonable ways as
> different from the original.
> Pursuant to GPLv3 §7(e): you may decline to grant trademark rights for certain names.

A **"modified version"** is any derivative of the Work, including but not limited to: changing
source code, adding or removing features, changing the interface or icons, altering default
settings, repackaging, or incorporating the Work into another project before distribution.

When distributing a modified version, you must:

1. **Use a name clearly different from the original.** You may not keep using
   "Qingci Craft Launcher", "Qingci", "QCL", "清辞", or any name easily confused with them
   (including variants, abbreviations and combinations in Chinese or English).
2. **Use the new name consistently wherever users see it**: program name, window title, "About"
   screen, installer metadata, distribution page and repository description.
3. **Not imply any affiliation.** You may not state or suggest that a modified version is
   published, endorsed, sponsored or supported by the original author, nor use the author's name or
   marks to endorse it.

### Verbatim copies may keep the original name

**Distributing an unmodified, complete copy is not restricted by this section and may keep the
original name.** GPLv3 permits verbatim copying and distribution, and these additional terms add no
condition to it — this omission is deliberate and marks the boundary of what Section 7 allows.

### Purpose of this section

It is not meant to restrict modification, but to let users **tell the original apart from someone
else's modified version**. A launcher handles account credentials and game files; when something
goes wrong, it must be traceable to a specific build. Modification itself remains entirely free,
including commercial use.

## 3. What these terms do not restrict

These additional terms **do not change any right granted by GPLv3**:

- run the Work for any purpose
- modify the Work and distribute modified versions (subject only to renaming per Section 2)
- use it commercially, sell it, or bundle it with a product
- obtain and exercise patent grants under GPLv3 Section 11
- sublicense, to the extent GPLv3 allows

These are terms explicitly permitted by GPLv3 §7 (items (b), (c) and (e)) and **do not constitute a
"further restriction" within the meaning of §10**, so the Work remains redistributable.

## 4. Notice in source files

GPLv3 §7 requires the additional terms to appear in the relevant source files, or a notice saying
where to find them. Every `.rs` / `.ts` / `.tsx` file in this repository begins with:

```
Copyright (C) 2026 luoqingci
SPDX-License-Identifier: GPL-3.0-only
附加条款（第 7 条）：见 ADDITIONAL-TERMS.md
```

## 5. Conflicts

If these additional terms conflict with the GPLv3 text, **GPLv3 governs**, and these terms remain
in force to the greatest extent permitted.

---

## Contributions

Submitting a contribution to this repository means you agree that it is licensed under GPLv3 and
subject to these additional terms (see [CONTRIBUTING.en.md](CONTRIBUTING.en.md)). If you do not
accept them, please do not submit.
