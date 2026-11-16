# Rights, ownership and distribution notice

Updated 15 November 2026.

Looking Glass is an independent, unofficial compatibility engine for American
McGee's Alice. **This project is not endorsed by or affiliated with EA or its
licensors.** It is not an official EA release, and no approval or licence from EA
for this project is claimed. The description "re-remaster" identifies the
project's aim; it does not imply an authorised rerelease of the game.

## Original game rights

**American McGee's Alice is an Electronic Arts (EA) property.** Copyright,
trademark and other rights in the original game, its proprietary software,
characters, story, dialogue, artwork, textures, models, levels, animations,
music, voices, sound effects, cinematics, names and logos remain with EA and/or
their respective licensors and other rights holders.

Looking Glass and its contributors **claim no ownership of those original game
assets or rights**. Naming the game, supporting its file formats or showing its
content does not transfer any of those rights to this project or its users.
Game names are used to identify compatibility. All third-party rights remain
reserved to their respective owners.

## What the project distributes

The project distributes its independently written engine, supporting setup
tools and documentation. **The project does not host, mirror or bundle the
original game's installable data archives, extracted asset files, executables
or gameplay DLLs.** The engine download does not include a copy of the original
game or a licence to its content. Compatible game data is required separately.

There are two important distinctions:

- **External downloads:** setup offers an optional download from Internet
  Archive, a third-party service. Setup defaults to local files; downloading
  requires an explicit selection and a separate entitlement confirmation that
  defaults to No. Earlier preview packages may still preselect the download
  when no local files are found. This transfers
  files from that service to the user's computer, not from a project-hosted
  game-data mirror. Neither that option nor a link, compatibility check or
  checksum establishes the uploader's permission or the user's entitlement.
  Use it only if you have the necessary rights; otherwise select compatible
  files from a source you are authorised to use.
- **Gameplay media:** the repository, website and project publicity include
  screenshots and gameplay/cutscene recordings that depict original game
  content. These are demonstration media, not playable game-data packages.
  The underlying content remains the property of its rights holders; it is
  not offered as open-source material or licensed for reuse by this project.

## Engine licence and third-party components

The [MIT licence](LICENSE-MIT) applies only to newly authored Looking Glass
code and documentation that the contributors are entitled to license. It does
not cover the original game, its assets or trademarks, or the gameplay media.
It grants no permission to copy, download, distribute or otherwise use those
materials. Supporting the original game does not assign the independently
written engine code to EA.

Third-party software retains its own licences and notices; see
[THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) and the notices supplied in
the Windows package. Nothing here replaces those terms.

## PlayStation Vita port

The PlayStation Vita port in the `vita-*` branches of the Anezium/LookingGlass
fork is an unofficial community port. It is not reviewed or endorsed by the
Looking Glass maintainers, and it is not affiliated with EA or Sony Interactive
Entertainment. Its packages contain no game data and no Internet Archive
download option. They do not distribute Sony firmware or Sony libraries:
players copy their own game files to `ux0:data/alice/base/`, and the shader
compiler `libshacccg.suprx` is loaded from their own console.

The Vita build links these third-party libraries statically:

- **vitaGL** at `dca4b9d143290d78ec043131a19be36c78cdc7b5`, LGPL-3.0-or-later;
- **vitaShaRK** at `df24065e65098b2d1ac533760109ad4367573f28` (tag `v.1.7`),
  LGPL-3.0-or-later;
- **pthread-embedded** (VitaSDK fork) at
  `11d2e5722d98c86f33c908fc47b2cf6e55205db5`, LGPL-2.1 for the core with an
  MIT-licensed PS Vita platform layer;
- **SceShaccCgExt** at `d3c83efceedeca3be863d0c1f19bb145daf1685f`, LGPL-2.1.

Their notices and licence texts are in
[THIRD_PARTY_NOTICES-vita.txt](THIRD_PARTY_NOTICES-vita.txt). A release VPK also
carries `THIRD_PARTY_NOTICES-vita.txt`, `CREDITS.txt`, the full licence texts
under `licenses/`, and `SOURCE-OFFER.txt`, which names the published source
bundle, the written offer, and the source route for the MPL-2.0 components. The
source bundle carries each library's corresponding source at the linked
revision, the port's object code for this link, the exact linker command, and
relink and packaging scripts.

### SceShaccCgExt modification

The upstream revision includes an optional intrinsic prelude in
`include/sce_intrinsics.h`. It is a C string literal that begins with the
markers "SCE CONFIDENTIAL", "Copyright (C) 2012 Sony Computer Entertainment
Inc." and "All Rights Reserved". The release removes that file, its build entry
and its load by the shader compiler, so no text from it is built, linked,
packaged or offered. It retains the standard-library load, optional caller
source, extension flag and other patches. The source bundle ships the patched
source and `SceShaccCgExt-no-prelude.patch`, with dated modification notices.

**Status: resolved by the owner on 2026-11-15.** The owner approved shipping
SceShaccCgExt without the prelude after the console check of that date recorded
45 of 45 shader-cache payloads byte-identical with and without it; the check is
retained in the release evidence, and the public project source is
<https://github.com/Anezium/LookingGlass>. This removal does not resolve the
other independent licence questions identified in the release audit.

### Licence evidence gaps accepted by the owner (2026-11-15)

Two further gaps were open at this release. The owner decided on 2026-11-15 that
no permission request would be posted for either of them, and that the release
ships with each gap documented and the authors credited. **That decision is an
accepted risk, not a licence grant.** It does not assert that permission exists,
it does not change the terms of the files, and it is not a statement that the
use is licensed.

- **newlib PS Vita port.** The release links objects compiled from
  `newlib/libc/sys/vita` in the VitaSDK newlib fork at
  `64aa7aa33d4f380451a1f100d19589226cdad334` (the VitaSDK build of 2026-08-25).
  `io.c`, `mlock.c`, `sbrk.c`, `syscalls.c` and `threading.c` carry no
  copyright or licence line at that revision, and `crt0.c` carries an
  attribution to PSPSDK (Copyright (c) 2005 Marcus R. Brown) with no licence
  text. None of these files is named in `COPYING.NEWLIB`. The notices keep
  those measured facts and claim no licence. The contributors recorded in the
  public Git history of <https://github.com/vitasdk/newlib>, with the
  introduction commit of each file, are credited in
  [THIRD_PARTY_NOTICES-vita.txt](THIRD_PARTY_NOTICES-vita.txt) and
  [CREDITS.txt](CREDITS.txt).
- **Red-black tree in SceShaccCgExt.** `src/shacccg_ext_oop.cpp` in
  SceShaccCgExt at `d3c83efceedeca3be863d0c1f19bb145daf1685f` contains a
  red-black tree copied from <https://github.com/Dynmi/RedBlackTree>, whose
  `rbtree.h` names its author as Haris Wang (GitHub: Dynmi). The upstream
  repository has no licence file and its files state no licence. The author and
  repository are credited in the notices and `CREDITS.txt`; no licence is
  claimed for that code.

The notices state no licence for either item. A reader who needs a grant for
these files must obtain it from their authors; this release does not provide
one. The earlier audit's open items and measurements remain in the release
record and in `tools/vita_licenses/newlib-vita-glue-NOTICE.txt`.

Concerns about the port go to the maintainer of that fork through its
[issue tracker](https://github.com/Anezium/LookingGlass/issues), not to the
upstream project.

## Using game data and contributing

Users must obtain and use compatible game files in accordance with the rights
and permissions applicable to them. Internet availability, a download link,
the age of a game or a lack of current retail availability does not by itself
grant permission. Looking Glass does not grant an exemption from applicable
law or the terms governing the original game.

Do not upload original game files, extracted assets, proprietary code,
decompiler dumps or personal saves to this repository or its issue tracker.
The [contribution guidance](https://github.com/looking-glass-project/LookingGlass/blob/main/CONTRIBUTING.md)
describes the source boundary.

## Warranty and scope of this notice

The engine is an experimental preview provided "as is", subject to the warranty
disclaimer and limitation of liability in the MIT licence, to the extent
permitted by applicable law. Nothing in this notice excludes rights or
liability that cannot lawfully be excluded, or restricts a third party's rights.

This notice explains ownership, distribution and licence scope. It is not a
claim of legal clearance or permission from a rights holder. Attribution,
non-affiliation wording and distributing the engine without game data do not
by themselves establish that every use or distribution is lawful.

## Rights-holder concerns

Rights holders or their authorised representatives can report a concern through
the [issue tracker](https://github.com/Anezium/LookingGlass/issues).
Please identify the material, its project URL, the rights concerned and the
requested action. Do not attach confidential information or original game data
to a public report. Reports will be reviewed, and material may be restricted or
removed while a concern is assessed.

## References

- [EA's content policy](https://help.ea.com/en/articles/security-and-rules/ea-content-policy/)
- [American McGee's published explanation of EA's ownership of the Alice property](https://americanmcgee.com/2023/04/12/so-whats-next-and-the-answers-to-your-questions/)

These references provide context; linking them does not imply that they
authorise Looking Glass.
