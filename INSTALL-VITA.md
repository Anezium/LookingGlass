# Install on PS Vita

Install the VPK with VitaShell, then close VitaShell and start the game from LiveArea.
The VPK contains the engine and original launcher art, with no game data.
The shader compiler `ur0:data/libshacccg.suprx` must already be installed
(for example using ShaRKBR33D).

Copy these six files from your own compatible PC game to `ux0:data/alice/base/`.
Keep the names unchanged. At startup the game checks that all six are present
and compares their sizes with the edition below. A missing file shows a message
with the folder and file names; press OK to exit and copy it. A size difference
is only logged, since other editions may work. The hashes below are the tested
edition, for checking your copy on a computer.

| File | SHA-256 |
| --- | --- |
| `pak0.pk3` | `6a63b59cd627902660bd4261461f904bf984e05891eb749d47bcc7fc8cedda62` |
| `pak1_large.pk3` | `eac773ca4012842c63738f348d03b2604f6d0798ad26536f7256c659ff7cdfc7` |
| `pak2.pk3` | `936d16c2311613c46b3167772b1c22aac693f96814cd70e3224a110884613864` |
| `pak3.pk3` | `a779bf348edcfb1d8711d1aad08537b30cf5794e6f8149168f4ca7a415aab8e4` |
| `pak4_english.pk3` | `ca30b395f770c0b5676d42b0ab81582f17976b526ed44c137512014a88672121` |
| `pak5_mod.pk3` | `cc36d55ed798fe9ff793c2cfb19551de0611acfa1f70cbf56b41caa068d22340` |

Archive download link: TODO-ARCHIVE-LINK
Use game files only when you have the necessary rights.

Saves and settings live under `ux0:data/alice/private/`. Back them up before
updating. Original PC saves are not compatible. Supplied private renderer and
simulation settings override the compiled release settings.

See KNOWN-ISSUES-VITA.md, CREDITS-VITA.md and THIRD_PARTY_NOTICES-vita.txt.
