"""Audit the Git index before a source commit. Read-only; never uploads."""
from pathlib import Path, PurePosixPath
import argparse
import hashlib
import json
import re
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
ROOT_FILES = {
    ".gitignore", ".gitattributes", "Cargo.toml", "Cargo.lock", "README.md",
    "LEGAL.md", "LICENSE-MIT", "Setup.cmd", "CONTRIBUTING.md", "SECURITY.md",
    "THIRD_PARTY_NOTICES.txt", "build.rs",
}
SUFFIXES = {"src": {".rs", ".json"}, "docs": {".md"}, "tools": {".py", ".ps1", ".java", ".cmd", ".md"}}
# Authored public website files only; media retains its exact hash boundary.
SITE_FILES = {
    "docs/site/index.html", "docs/site/styles.css", "docs/site/favicon.svg",
    "docs/site/sitemap.xml",
}
# Authored build configuration and named presets, never a general data allowance.
CONFIG_FILES = {
    ".cargo/config.toml", "configs/PL-def-r3.json", "configs/vita_renderer.lean.json",
    "configs/PL-r21-human.json", "configs/PL-r21-human-levers-off.json",
    "configs/PL-r22-human.json", "configs/PL-r22-human-levers-off.json",
    "configs/PL-r25-human.json", "configs/PL-r25-human-levers-off.json", "configs/PL-r25-human-measure.json",
    "configs/PL-r3d-baseline.json", "configs/PL-r3d-actor-prepare-off.json",
    "configs/PL-r3d-native-scale.json", "configs/PL-r3d-preview-every-frame.json",
    "configs/PL-r3d-film-bypass.json", "configs/PL-r3d-audio-direct.json",
    "configs/vita-sim/vita-sim-defaults.json", "configs/vita-sim/vita-sim-all.json",
    "configs/vita-sim/vita-sim-puff-cap.json", "configs/vita-sim/vita-sim-puff-collide.json",
    "configs/vita-sim/vita-sim-puff-collide-none.json", "configs/vita-sim/vita-sim-puff-sweep-every.json",
    "configs/vita-sim/vita-sim-anim-cull.json", "configs/vita-sim/vita-sim-anim-far-distance.json",
    "configs/vita-sim/vita-sim-anim-far-every.json", "configs/vita-sim/vita-sim-mover-far-every.json",
}
# Exact path and SHA-256 for release files outside the general suffix rules.
# These were reviewed for the Vita release; a changed byte is a new review, not
# an automatic pass. No directory, extension or size wildcard is granted here.
PINNED_FILES = {
    # Release documents, notices and review fixtures.
    "CREDITS.txt": "ccbc4212f9fc9a47cbcc3d0db285e3335a20041bc23996ca0f8169b289331b01",
    "THIRD_PARTY_NOTICES-vita.txt": "ca7b0ec36a346cdc5ec6e17e9b3f9e3743bdac7f941b82e6928607d2b0ed222b",
    "docs/NATIVE-TIMING-PAIR-SPEC.example.json": "0f95e537ad012d54dfba38c63cf3f33f77096688a6a9b296c8744a463a081d3b",
    "examples/transition_texture_lifetime.rs": "8a5bca77466dcd7ded278701f0e0a176118bb8379af71c68021883c6b0d93768",
    "src/frame_total_gl_names.txt": "662e07e8c1886b29626caa105e9d84d25efaec21ad7f61bf5aedacc909aaf6d4",
    "tests/native_allocator_observer.py": "f465d7592afa36fa4b61454b2b18f2bc42afd7d28f6cdfc6deeda27166b04818",
    "tests/native_allocator_observer.rs.in": "9421c3afc5cf45f208a21defd73434551a7b0826ba25091d8f1800a699683218",
    # Reviewed runtime and measurement configurations, one entry per preset.
    "configs/PL-a2-record.json": "6b8f7fb81974848e2645bdd69b6d02ae6c37c4363b3e6a62b95d35696c20fffb",
    "configs/PL-a2-replay.json": "3ab0fd67300807918873724f9562097d89ff34d15e5967a8bf1560d0a08de1c4",
    "configs/PL-a3-loading.json": "e7de18aaa0c3ce30884336976e69ee371ae36797d97135d5e396f2d9dfffaad3",
    "configs/PL-a3-water.json": "eadfed36c39a1dcde778a5bde726277488a6da76e9e94017f046470db9afd26d",
    "configs/PL-a5-hatter2-control.json": "59a25afe2e88249bbc89b69035139f4f437ad97fc16468d65992bd6edff50986",
    "configs/PL-a5-hatter2-unlit-exact.json": "3287cd2bec27705cb51ffb19372491675af3e1e6a29c4c38b76e3347de3fd3f2",
    "configs/PL-a5-water-control.json": "f6e099a962f83eface2725c736b27a262b6429ecb5dd13ba32aba10fe06d7a5a",
    "configs/PL-a5-water-join.json": "9cde7f12ad2f7c9a5b206065e31476d5f66e26df70287397595eb219f388b881",
    "configs/PL-a5-wchess1-alpha-model-pvs.json": "b4f6bd79eed57db681f89dc7b69432854b014446191f1ed2e3607fce4f6bf214",
    "configs/PL-a5-wchess1-control.json": "d79f397f5fbccf57c6bdb30a2629b7f2b8cb471a7087cbf70e164fa01c401c40",
    "configs/PL-a6-load-off.json": "e0784ff285ac31159e940d08100693af4239a27164a9fedb8220bf0ce664baf0",
    "configs/PL-a6-load-on.json": "201cd87ca477edcfc18e2966e7391e77fb5f14f96a98269885b6a875073999d4",
    "configs/PL-played-rc3.json": "def605c0bd92a38281cb86fd9c20e36a794946999569e3a1f4131e1d0bba7c33",
    "configs/PL-r3e-baseline.json": "1428a7c9cc644d71ab41cc5d9b442278b5792ebdacf4b8712b6c456b7b5ace07",
    "configs/PL-r3e-cushion-2.json": "6b435a74a47c20a294946bdc437cd3d7ef484edb4b48834fe47b0c91cfdb50ed",
    "configs/PL-r3e-no-cap.json": "2df7ff74bb7712a296133bbef7a6f4afb9b4c1461248f234b5c92225c85377b7",
    "configs/PL-r3e-no-preresample.json": "703a830d0e8ea1ae36180d2e5f0696a5c10dafaa24019b8ad03bf60282a54f65",
    "configs/PL-r3e-parameters-sample.json": "4d3cf4550a68cd34d9c6f244408f02b8304510aca4d81aa9a8ecc2367f9ccebc",
    "configs/PL-r3e-r3d-control.json": "3223151aa12c478fb320c365d62e3b9fde9661babb8714b2e6e1e1c307d493d9",
    "configs/PL-r3e-worker-probe.json": "b647a775f55514b411cf569926a34861a9648108d93bb0eb5caa9f5ad33c7fea",
    "configs/PL-r3f-candidate.json": "9da996d5d973b502594cd9ca801d9aa4fa548ac3dc5f8fa42bb74bc39c64960a",
    "configs/PL-r3f-cushion-8.json": "59a8da86293b9eec10330cd34e5d0f3e901066a40550b8fd69f2b499b25b99dc",
    "configs/PL-r3f-r3e-control.json": "18d3e4130dba6dfabaeeb2e66a6e54549003850a8aecacffb580856639763e82",
    "configs/PL-r3g-reference.json": "af184a8f6947deb4a0b8d4906ccd03a9c7cc2732fd4dc3e36a617bb4b58ede2e",
    "configs/PL-r3g-round2-off.json": "c177283a815c53df48a1165ecfc18e0cc6599b9baddc597077a82f3c3d3e7c69",
    "configs/PL-r3h-capture.json": "01b2b10de2c0e072fc9b9c7e340aa82fbe6fe667bc319f17b655b4b65af92bbf",
    "configs/PL-r3h-reference.json": "4a219a90e623f1852fa060eb3c3b5271b179289ef238d50ea6ffc665e39cfca7",
    "configs/PL-rc1-a3-direct-flip.json": "bb535ec966fb65220d715e812ec75bb351ab46189ef4b4faa6826e4fac5b7b5d",
    "configs/PL-rc1-a3-direct-fx-on.json": "cff0ab7b5faa9a9a14f0741d7c605f94518d56b37ddb677d258bfd6ca23a7fc3",
    "configs/PL-rc1-a3-direct.json": "ad864359c5dfcdb5cad4be7285a9718eeeaa4be8663f3f73aed2bf1811ca3926",
    "configs/PL-rc1-fx-on.json": "d4b182926ef8e5792bcb4bdf6659a193afc70e72704d3ee678e3aa09e96906a1",
    "configs/PL-rc1-levers-off.json": "9526fd5c8889da960c3db399a74ca48c114d6323513c5b33923bd8523904bdc9",
    "configs/PL-rc1-sky-on.json": "94ebebf47e836df775db7fbcc338d5792bec43ed8b4d359cf7c37b81fa5a9f8f",
    "configs/PL-rc1.json": "0aa425e8af361159cd323695a9977ae8d5786277a40d2fdc791f42a17b90e1d3",
    "configs/PL-rc10-hatter2.json": "999287ede6c96a19194c95d4fa3695de5fed5957233d121509597953b2640e3b",
    "configs/PL-rc10-release-play04.json": "4bf62178a0ccfefe5314feafc7a2fdf9b83777f66e8ee77684eb34a606616477",
    "configs/PL-rc10-release.json": "78547291fe3455184564a2437addec154ba8d056d4cd4dc4e4cead80f86624c6",
    "configs/PL-rc10-skool1.json": "a5500e69b10887eea2bece2f4aa5492fe7dccd07115b6f235d3410988011f89c",
    "configs/PL-rc11-load-funhouse.json": "243f8982cf66b9a45a55b6f716f5d517d9d26669e7f0dbc34f7b5ba5a3a04b11",
    "configs/PL-rc11-load-gvillage.json": "f84e4ce7886017208292a0847f467c65fc8fe1f4af07665b29b9ab2aa73e8724",
    "configs/PL-rc11-release-play04.json": "afc6cb38547f2dc5b0b6ee34ee33b4ed4e879fb838678b5c55c77c58d0aee7f3",
    "configs/PL-rc11-release.json": "00d1de6a0e8a692c4a32b7a5289e2990dcab91e422b70d76925bc8657fa6a24a",
    "configs/PL-rc12-load-funhouse.json": "5bda41f40994bab65995d732afe40421226919bb958a781e0de582afedecbc40",
    "configs/PL-rc12-load-gvillage.json": "1bb5c22c931d2af4d02a1d1257cd3047100d803b9b05d2524ab3607cd315f3a4",
    "configs/PL-rc12-release-play04.json": "0182cbaa4462376f1d417f058661a98bd62eb7c8cfd9cae7ca7bd4d5349a6476",
    "configs/PL-rc12-release.json": "4b1a1503b00b7826258a54fff0e69e5bdbd3a609f440bc13aaa2926aae9cb9f5",
    "configs/PL-rc13-release-play04.json": "5f62a50a1d69f27d37bb1a8b39f39462e7fab491daafc81aa47d17b8d4783f7c",
    "configs/PL-rc13-wchess1.json": "ae7a81f50dd8d3ee090cfb5a68fdbce2b0d1f90b6847aa4469e309905f205cc5",
    "configs/PL-rc2-a5-add-cap10.json": "ed705eeedfdf35a2da090da7fe5d0cdc2d20888553ae69c01776c871e0e87d32",
    "configs/PL-rc2-a5-all.json": "144d14f89a2a5fb943903252f40dfa68d6663e0493011640183a255228bc8aac",
    "configs/PL-rc2-a5-alpha-lite.json": "b3dd2dee583c37de9afb02ade4760fb5c104de3a5ee6a98eee8104e13aeca266",
    "configs/PL-rc2-a5-exact.json": "9b5fa7f67e7aec36176430abde72f490c28f7263f244c93bf4e02d9fc6f0988d",
    "configs/PL-rc2-a5-fill.json": "21aa1aa9a65ae8eefbee0d1fb96a20cec752c2464e997fdafbe5aac82f4ba901",
    "configs/PL-rc2-a5-fx-lite.json": "6d84eb2c879823ffb1ff36e242f6e892bd8988f8148a435bfc57b6963bc9a393",
    "configs/PL-rc2-a5-packet-frustum.json": "2bc03e93b05efcd45ad9acee0d134f16bb85d8bd7afb2a9abf3d99c8d911894a",
    "configs/PL-rc2-a5-packet-min6.json": "388a4a0151f98457f6be169162beb0484aa0ef2da9922564bd0e626e10f6ba64",
    "configs/PL-rc2-a5-prop-frustum.json": "5def30b829becd2c404b0f7da8f8ea78c0b06c8acd363da50ca034bbdbe1c9ce",
    "configs/PL-rc2-a5-texture-sort.json": "9679ef1ed0f34525515c5539b822270297ff02738466ad67f53c6021d4e44c14",
    "configs/PL-rc2-a5-water-drop.json": "6e17a53a19123f5de8f13d859ba3b4952b757e3db28e0e3d548f165a84050609",
    "configs/PL-rc2-a5-water-merge.json": "32f8bbfcad5016e70cd0f9c38233701795140c6ebd1a409d107feb8ae578ab9a",
    "configs/PL-rc2-a5-world-min6.json": "1af0df9e3a524dca024ea9891657233cbd4c278579643479d4a3722b9a2e6c01",
    "configs/PL-rc2-a5-zero-alpha.json": "200357354702323e8c2caca644a2da6602066556f41da6423f6ba52c42c3fd30",
    "configs/PL-rc2-direct-flip.json": "bb1d010f1d4c402f8966dae5c8ebfced6b5f38ba1e45fc3249b02022a0d8a6a5",
    "configs/PL-rc2-direct.json": "b860f6ccada22496df56c2b0bccefd12752f4d54da69572c29fccd5f335624cf",
    "configs/PL-rc2-loading-off.json": "cffa79c2ce39cfe891ba159d0da0152a00809d497dbdeb86f18f2092c1d1a3ca",
    "configs/PL-rc2-texcache.json": "c9b55c0d286692f3eeab11777fd76f17485980414f99aba297c7a66d5eb6b3c3",
    "configs/PL-rc2.json": "e7de18aaa0c3ce30884336976e69ee371ae36797d97135d5e396f2d9dfffaad3",
    "configs/PL-rc3-audio.json": "8cdf30a3e765ed665817b65813dfaa8df680e05ece9ecb0630c58844fcc9bbf5",
    "configs/PL-rc3-blocked.json": "674166c38e9efa73bb3bf8a6b464f6701e04c39bbcc1acf96e350502fd00f979",
    "configs/PL-rc3-detail.json": "1c2e567b79c8306dbafa61c9bc72fc904dab1be127a1bd5e227ce6ece4b4722c",
    "configs/PL-rc3-fidelity.json": "99990ccb2ebcd5597953f63f494b3f32fec80f2714f447e8095caa0872c3a2de",
    "configs/PL-rc3-fight.json": "466030de100717c01e72a04910a1af6eb0c1b8a50e8c5b49bf45981e7bee5922",
    "configs/PL-rc3-fill.json": "2b4c378adb316ade9a3bb0632cd66ab0cb5c300635a46069a7a3c989c197c8d6",
    "configs/PL-rc3-fog.json": "33ab0b4db101c54d7278fb431595580ac3b654774b5393b1fcd4e6e69466a64d",
    "configs/PL-rc3-near.json": "cd2f3baec80a73778518ccdfea2492dc15d4e8b2fbadbc7b39de213d92e4c302",
    "configs/PL-rc3-record.json": "b900621719e35893b70d4cd8fb94bff57103656d84d5bd6ecfad63f774bfcd0d",
    "configs/PL-rc3-replay.json": "4af57b28367dcf6e5555d453415dfa79becb4f7a4e7ce7db8333cd2e184df27a",
    "configs/PL-rc3-resolve.json": "200357354702323e8c2caca644a2da6602066556f41da6423f6ba52c42c3fd30",
    "configs/PL-rc3-sky-depth.json": "1422897409879f357c33465f785c96c081b674313610ee90c19e88add11ef3e3",
    "configs/PL-rc3-sky-finish.json": "f37d3ac2b4841a0c190344c523f9008899c2b286692ed0c8bd8f8b7e0c414619",
    "configs/PL-rc3-sky.json": "365e73bbe064870e6b2c14c68a132d3b4714327441a3b0d16e232986efdfce94",
    "configs/PL-rc3-spike.json": "58aacd40de00522a967f704be27f00f2a94f5b7010354a20afb4119565a5c638",
    "configs/PL-rc3-water.json": "951ada9b59f6e256e596c39d09c28fb2b26972ed10c79d9f65d46c9744050c08",
    "configs/PL-rc3.json": "55211bf7e9a2578f9337aae887602c163936bfd4566510f6356d763d19eee44b",
    "configs/PL-rc4-exact-fog.json": "4e69688a44c5e3e2bdae98ac42ab77a0e62e21cf6a5878498e5f4c22408c38ec",
    "configs/PL-rc4-fill.json": "4cec98c19fc2a6389b23b6fdac6c907703b582c056836086fd33962b2a854783",
    "configs/PL-rc4-fog.json": "d115a8e4dd116dfc091698d3df93148e8903ffe07b960f850b12fae0b6433118",
    "configs/PL-rc4-record.json": "2cdd91823f248e8e9bcb4c61a8cdb645d3ec620dbee13317088fd588917c15c6",
    "configs/PL-rc4-replay.json": "57fcb5d3bd6e9242a596be8a2baf6d52ac9cdf01d4e68b928513fd2102df5d27",
    "configs/PL-rc4-river.json": "45dceaac8f4cd1d0b7a21711e65e3bceaf2a0ecc8408fdba1a0f52683c1338fe",
    "configs/PL-rc4-sky-finish-fog.json": "4442ca2f5ab10a19b0989de332194e0ca4ebaf2b90eecdf2fcfc64190304b8d2",
    "configs/PL-rc4-water.json": "7fc3d02c3adcd0a11284196231737f358daacbd70c75f34fe384bffd0bc46973",
    "configs/PL-rc4.json": "fa60ef8ae35b16366a80ac155d7f9fb081b83d0bfd1b86e31c91386ede2387be",
    "configs/PL-rc7-release-play04.json": "57fcb5d3bd6e9242a596be8a2baf6d52ac9cdf01d4e68b928513fd2102df5d27",
    "configs/PL-rc7-release.json": "593778fbe24338cbb2c8d00a8504286d5c26018dbc2bc663500221095bfd2b80",
    "configs/PL-rc8-release-play04.json": "39663cf8c10457d89d0106c714c67efe3f4776b9c50d31d2424215475d0dbf70",
    "configs/PL-rc8-release.json": "a5d21beec0d15648cc94f209bbe0adf1fa387430c0df18a1a5e10ad2756f6f2b",
    "configs/PL-rc8-spike-play04.json": "05ea8f24e724f7af285cdaf71bb6d2911093ea229e26384c4915e681f0d074c6",
    "configs/PL-rc9-release-play04.json": "1526df21bc66e8d4651006c2df5871f889b145c0295dcf5c3e3cc863cc0d694e",
    "configs/PL-rc9-release.json": "9dbf482cf40ff440dd74ecc5c6be45aaf5c3cb0b06da3bfc0fe47a4b76466cac",
    "configs/PL-release.json": "216f8e8b34b24de7dab871dd16fb204e67a08d1cc1b9e0ea16ce86552a10d850",
    "configs/RELEASE.md": "579c72a75b408f1e345c1ed089c5fea6ad4a4e711df9da7291b86cfb1c9a96fa",
    "configs/vita-sim/vita-sim-release.json": "fa64b0c5b79c52d8e028f7fc9d0d3dd2eec57b7641dfc8f76b34212bdcb3334b",
    "configs/PL-rc15-release-play.json": "580f73ba3e3bd166fe2362e2e2a9ab229ad2080d0de8f1036b24d80975585c70",
    "configs/PL-rc16-release-play.json": "00b79a6797366e4aca9871883eef74a3aea64d6963f18bac8010c73d6d689677",
    "configs/PL-rc17-release-play.json": "728de54c49ec583aa3941de0ee16ca4f9f672795c5e5528f584d17f71db82749",
    "configs/PL-rc18-release-play.json": "61ad98e302e6c9ddd0ed9994df5c6b230242cf7315b640840f63ddf84c242189",
    "configs/PL-rc19-release-play.json": "40f824124d6d2a916eb59b800b469125b3a015324ececbc282917bf971d8e0e2",
    "configs/PL-rc20-release-play.json": "5a80f4d5636f55953909db87ba1452ded9f3cc74ffb2bcbd6e4389c9027022d0",
    "configs/PL-rc22-release-play.json": "5934d29d042279f066de5745d4e3d7d58c86ad3a02ad439b363fa2130992b5ee",
    # Tooling, licence texts and test fixtures.
    "tools/config_record/fixtures/host-bad.json": "0baa0c4b95907662865e70a9946f546c486fca6e3808dc64bab650a16b2b5912",
    "tools/config_record/fixtures/host-bad.txt": "9744a970a2ce54da3cfc3de774339a1989d6b1e4207fafe0b5686329089d0da5",
    "tools/config_record/fixtures/host-omitted.json": "20d99e4607c3551380fcba54dc663f047d8f62548e0cc0a85db6eb5a1e6c573a",
    "tools/config_record/fixtures/host-omitted.txt": "1447687cd3e1ebb2bf978c4a3d46f6f5dc7bcb9209acb6b44e543692f8908548",
    "tools/config_record/fixtures/host-zero.json": "d63cb421ecd741f53abb6a183137b3241a00da2dc1077c7e61e7b612ea631d0c",
    "tools/config_record/fixtures/host-zero.txt": "9be027ada0b8ac4a93d2f7621381967707f0be131864ef3a3a25244c74c0e764",
    "tools/lever_matrix/CONSOLE-PROTOCOL.json": "3732ecefa0742ac5c8dbf3481a2e798961b04b81360dc4010a50a3f5fde23639",
    "tools/lever_matrix/configs/PL-audio.json": "2f330c05d1af3df7629ab04d45b67d1fdc0bfc6d765462cbf99df0f0a89604c6",
    "tools/lever_matrix/configs/PL-baseline.json": "c6a3060c3df39319b197452e92ec3df5125f7b49bcedf9b98e19a2a575227e07",
    "tools/lever_matrix/configs/PL-fx.json": "0d39c347a47bede5ef6e71f30bc5aa4f5aebe17638e7a07fd226d947e322c643",
    "tools/lever_matrix/configs/PL-world.json": "24f0306386a596400c90706ae6a80dc5730b56ec480988dda566d994588ff075",
    "tools/lever_matrix/configs/native-timing-disabled.json": "4a30c9be3f878a9b2ecf59f5e0ada934c9d7e390f09c232597a46d5b4e929a50",
    "tools/lever_matrix/configs/profile_mode-full.txt": "0e716a5fef4e6dc1bcfff22ad52f73ca4eee3f4ea8292f4a1918daa32592889f",
    "tools/lever_matrix/configs/profile_mode-fullbench.txt": "a03534216bd98b620f9589411094b185660919adf5a02ef34672f718b38a3cce",
    "tools/lever_matrix/configs/vita-sim-all.json": "fa64b0c5b79c52d8e028f7fc9d0d3dd2eec57b7641dfc8f76b34212bdcb3334b",
    "tools/lever_matrix_r3/CONSOLE-PROTOCOL.json": "b02876d83c542856fb25941727b630217ad245569749fa8d3d1d4c333e1f9aac",
    "tools/lever_matrix_r3/configs/PL-audio.json": "2f330c05d1af3df7629ab04d45b67d1fdc0bfc6d765462cbf99df0f0a89604c6",
    "tools/lever_matrix_r3/configs/PL-baseline.json": "c6a3060c3df39319b197452e92ec3df5125f7b49bcedf9b98e19a2a575227e07",
    "tools/lever_matrix_r3/configs/PL-fx.json": "0d39c347a47bede5ef6e71f30bc5aa4f5aebe17638e7a07fd226d947e322c643",
    "tools/lever_matrix_r3/configs/PL-world-material-sort.json": "4cd69c3fb50e9f5aacbb078afe89f94368f0f6aa96cee486f5f03735cd95abcf",
    "tools/lever_matrix_r3/configs/PL-world-static-merge.json": "3d034533b83e62c7679984b2bf63e0e15f2cec134c6a7f2aa0d91d2e8c8b044f",
    "tools/lever_matrix_r3/configs/PL-world-uniform-persist.json": "649bf2712e54e832fbbdf542c63e7b5785af83da9065c0b3a43130a20e85d285",
    "tools/lever_matrix_r3/configs/native-timing-disabled.json": "4a30c9be3f878a9b2ecf59f5e0ada934c9d7e390f09c232597a46d5b4e929a50",
    "tools/lever_matrix_r3/configs/profile_mode-full.txt": "0e716a5fef4e6dc1bcfff22ad52f73ca4eee3f4ea8292f4a1918daa32592889f",
    "tools/lever_matrix_r3/configs/profile_mode-fullbench.txt": "a03534216bd98b620f9589411094b185660919adf5a02ef34672f718b38a3cce",
    "tools/lever_matrix_r3/configs/vita-sim-all.json": "fa64b0c5b79c52d8e028f7fc9d0d3dd2eec57b7641dfc8f76b34212bdcb3334b",
    "tools/lever_matrix_r3/stage-audio-full.json": "490bf741e55323293de1ba0e3f836ccf108d111377775bb851b966730e31cd07",
    "tools/lever_matrix_r3/stage-audio-self.json": "7234ac84036a7ff902681a51e0b58fbaf7f8a1ec838b5853fea412623fa51b45",
    "tools/lever_matrix_r3/stage-baseline-full.json": "d70d5d49b69ebffc288079a4a2b1fb597f1b5d435c794456aac223876c7029fa",
    "tools/lever_matrix_r3/stage-baseline-self.json": "b52a628269e683b3276c8583dee48016e017deb3434bf0c102089d1d72a6ea34",
    "tools/lever_matrix_r3/stage-fullbench-mode-only.json": "2481052499931b016218f824a3709af2f9be84842101d649b53408eaffaf2d3c",
    "tools/lever_matrix_r3/stage-fx-full.json": "703392a1b620f1757d5022f74cc81f450c13e4e949ec1197757bab5206e5b8a7",
    "tools/lever_matrix_r3/stage-fx-self.json": "853443827c812e03763385b9e3ea3a6e5b81f28071c45844fcde79e2c841a738",
    "tools/lever_matrix_r3/stage-world-material-sort-full.json": "456a44d00773fd4385614aebe49eda7ed94a0bccbbe34671142aa3ee95e8df0a",
    "tools/lever_matrix_r3/stage-world-material-sort-self.json": "b5dc333e618b1c147bf299fc1bb30794f7f7bd7f1dc85b51ebc7f8b445e256e7",
    "tools/lever_matrix_r3/stage-world-static-merge-full.json": "b7c806771709a5e2f3c3c93edcba9c2ad0a193870f6e36ce0f4c4180cb9f858d",
    "tools/lever_matrix_r3/stage-world-static-merge-self.json": "b75369ca89b99def8593aba266106c44dbad3ae795a1ec1c991b0b88c8432fde",
    "tools/lever_matrix_r3/stage-world-uniform-persist-full.json": "a0ec07c9a8aa928fa9846f7f89a29f0adc2b6957b1be6eb9f32a8a5ead1e9f93",
    "tools/lever_matrix_r3/stage-world-uniform-persist-self.json": "2b2bf704b904dcd4f8ce177bb77c236b84ebaecd95595edd0159e79f5362307e",
    "tools/lever_matrix_r4/CONSOLE-PROTOCOL.json": "690e10bf1b69f8de2429ecdcf4cea5cae5de2e1147ba519d6eea6f620719bf05",
    "tools/lever_matrix_r4/allocation_probe.rs": "8c9380e820087d54d17becf6b00a501ff1cd7409f994cec47542b2cc4c3ddf4f",
    "tools/lever_matrix_r4/collision_probe.rs": "27c91d489432a4f49d712501f4d30cf63e5a945483801f7677f9d84334701c87",
    "tools/lever_matrix_r4/configs/PL-audio.json": "2f330c05d1af3df7629ab04d45b67d1fdc0bfc6d765462cbf99df0f0a89604c6",
    "tools/lever_matrix_r4/configs/PL-baseline.json": "c6a3060c3df39319b197452e92ec3df5125f7b49bcedf9b98e19a2a575227e07",
    "tools/lever_matrix_r4/configs/PL-world-static-merge.json": "3d034533b83e62c7679984b2bf63e0e15f2cec134c6a7f2aa0d91d2e8c8b044f",
    "tools/lever_matrix_r4/configs/deterministic-audio-config.json": "593b93d878ba8c313e3bcb3eb9ba368672b1c3189565a3ded2c57589a69155ac",
    "tools/lever_matrix_r4/configs/native-timing-disabled.json": "4a30c9be3f878a9b2ecf59f5e0ada934c9d7e390f09c232597a46d5b4e929a50",
    "tools/lever_matrix_r4/configs/profile_mode-full.txt": "0e716a5fef4e6dc1bcfff22ad52f73ca4eee3f4ea8292f4a1918daa32592889f",
    "tools/lever_matrix_r4/configs/profile_mode-fullbench.txt": "a03534216bd98b620f9589411094b185660919adf5a02ef34672f718b38a3cce",
    "tools/lever_matrix_r4/configs/vita-sim-all.json": "fa64b0c5b79c52d8e028f7fc9d0d3dd2eec57b7641dfc8f76b34212bdcb3334b",
    "tools/lever_matrix_r4/manifests/stage-audio-full.json": "815b3963c11a6e022cf7c18fab16e54c031d5fb53eca330d2d2d37cb78a4ae96",
    "tools/lever_matrix_r4/manifests/stage-audio-self.json": "cef4138ed17e35cd74a94d15d44380dd691796f1caa4aad74921092b639a72c9",
    "tools/lever_matrix_r4/manifests/stage-baseline-full.json": "bbf3317ada71446d514b171598e32b0e2e74edd57cd34b5b5020b955b4a69779",
    "tools/lever_matrix_r4/manifests/stage-baseline-self.json": "ec7168323e2dc492f2851530776af718f55d2824c9e54dae6e8b68204b47cc85",
    "tools/lever_matrix_r4/manifests/stage-fullbench-mode-only.json": "cf3cdeb01e66d91a94ef06a324d098728bad48794f387fec16d1dc3ae2165827",
    "tools/lever_matrix_r4/manifests/stage-world-static-merge-full.json": "e38fc5d420795dce9a42df136fb87b78e15d62391eefbdcabd15357fcb43b756",
    "tools/lever_matrix_r4/manifests/stage-world-static-merge-self.json": "6bdd4799eadea090f970c0190b9e895f8d641f0eff5557b1f3753a0ab89a332b",
    "tools/lever_matrix_r4/proof/log-integrity-fixtures/R3-SOURCES.json": "9045b86738c5f5513484c8e7c5e43898252b6ea5231a72600b70471b2395604c",
    "tools/lever_matrix_r4/proof/log-integrity-fixtures/complete.txt": "a8480a7502704af7f52509258928850d7c284dc1fc4ea68285f29aa98a56eb39",
    "tools/lever_matrix_r4/proof/log-integrity-fixtures/longer-A.txt": "28dc262649cf944bb29ac926ae145156ec10450cb5915bc185a514ecdf0736ae",
    "tools/lever_matrix_r4/proof/log-integrity-fixtures/negative-2-A.txt": "28dc262649cf944bb29ac926ae145156ec10450cb5915bc185a514ecdf0736ae",
    "tools/lever_matrix_r4/proof/log-integrity-fixtures/negative-2-B.txt": "2a0e23f3a42996dd0daf0a69de7e081065f1ab246ffdb9dcdfbde38f78f023b5",
    "tools/lever_matrix_r4/proof/log-integrity-fixtures/nested.txt": "5235e76f4ae0f738803469ec2cbe1831dd6309eddd5713e2d3800b4b747ffe6e",
    "tools/lever_matrix_r4/proof/log-integrity-fixtures/orphan.txt": "ced8a981f252219b8ff32111a357379ae3c67c89cebe889dfa66b444e55dea52",
    "tools/lever_matrix_r4/proof/log-integrity-fixtures/s4-audio-pandemonium-1-A/runtime-log.txt": "8f6f20e0146f42b572376fd2f19e31b68b606a9b388fce5c794b99aa8deefc36",
    "tools/lever_matrix_r4/proof/log-integrity-fixtures/s4-audio-pandemonium-2-B/runtime-log.txt": "81c53a5af03264ae84ac699828b17f25a7de126c105508537e68ff06b84025f3",
    "tools/patches/SceShaccCgExt-no-prelude.patch": "6c7b7053bbf559d8680d7c2bb79c3495c7b22feff6411c3ae08143400db79cd6",
    "tools/transition_mem_r11/CONSOLE-PROTOCOL.json": "05a9c5449963c81e7b318d3c50cbb6d54dfb519ede72918fb919a7bad1a70b61",
    "tools/transition_mem_r11/configs/PL-baseline.json": "a3105263db330fc3bf628d2a7d8cdb501328359d31ba7fa4f9bd3908e83c5480",
    "tools/transition_mem_r11/configs/PL-stack.json": "07e154450ef09326380754d8e29e6fab0a9982ffd235678e05b189de5fd6f9b1",
    "tools/transition_mem_r11/configs/native-timing-disabled.json": "4a30c9be3f878a9b2ecf59f5e0ada934c9d7e390f09c232597a46d5b4e929a50",
    "tools/transition_mem_r11/configs/profile_mode-full.txt": "0e716a5fef4e6dc1bcfff22ad52f73ca4eee3f4ea8292f4a1918daa32592889f",
    "tools/transition_mem_r11/configs/vita-sim-all.json": "fa64b0c5b79c52d8e028f7fc9d0d3dd2eec57b7641dfc8f76b34212bdcb3334b",
    "tools/transition_mem_r11/scripts/transition-log-fixture.json": "b5749d67a086c497b1231f8287269c40b3cf1f375fe7b352edd915b86ce14a27",
    "tools/vita_lgpl_bundle.sh": "6a4a3be84798b4ddc7a178cb00728510b7d09eba4d1429a838e04351ee0abc2b",
    "tools/vita_lgpl_link_capture.sh": "6231b679a0e7cc17b5dea37a263405edf02ec759e15b3707b7ed2b3a2ea3fe85",
    "tools/vita_licenses/Apache-2.0.txt": "074e6e32c86a4c0ef8b3ed25b721ca23aca83df277cd88106ef7177c354615ff",
    "tools/vita_licenses/GCC-RUNTIME-EXCEPTION-3.1.txt": "28e85c5aa4af9b4f1dfe6b4817aa3eefb3eaaee7fd735045016f29ccf50276a1",
    "tools/vita_licenses/GPL-3.0.txt": "3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986",
    "tools/vita_licenses/LookingGlass-LICENSE-MIT.txt": "8e8860a5d3823a31624ca7c8ea27681098be8bbb94778742938b6fbf30fd2f25",
    "tools/vita_licenses/MIT.txt": "b85dcd3e453d05982552c52b5fc9e0bdd6d23c6f8e844b984a88af32570b0cc0",
    "tools/vita_licenses/SOURCES.txt": "2ffc4928deed415ea74d6f919041cd4c414e336887ff0db32f05c2a99a410a9e",
    "tools/vita_licenses/SceShaccCgExt-LICENSE": "20c17d8b8c48a600800dfd14f95d5cb9ff47066a9641ddeab48dc54aec96e331",
    "tools/vita_licenses/SceShaccCgExt-release.json": "5b00e203c32c75dd7e794ea021c8c53ae8c8009e168b25ec97d3600fc7817dfd",
    "tools/vita_licenses/dasp-LICENSE-APACHE": "756c8d2ab2dc24f256e1455b5f03937f4933d83d7bc9f2d55009fc962377d512",
    "tools/vita_licenses/dasp-LICENSE-MIT": "b1d6df41ed3aa96806e74c729444d7c121d90e6660a6aed01d298e03fde475a0",
    "tools/vita_licenses/gcc-COPYING.RUNTIME": "9d6b43ce4d8de0c878bf16b54d8e7a10d9bd42b75178153e3af6a815bdc90f74",
    "tools/vita_licenses/lgpl-libraries.json": "77c38d3b5afa2be354b19a3533e43ce09c5b751f008af0fbf022f80cb1d8a635",
    "tools/vita_licenses/math-neon-LICENSE": "a0885b0834238a325f80903f94e8f41de46e241a82f689e7bb7bd89498eb944c",
    "tools/vita_licenses/newlib-COPYING.LIBGLOSS": "39859c73b5694929982241326d0508f8687708e4f3c15b733a9859ab62f94bb6",
    "tools/vita_licenses/newlib-COPYING.NEWLIB": "60c00eccf8c5bac75897ead93b96c0c73765b3d2f8e413e98a9daaf45ecdb244",
    "tools/vita_licenses/newlib-vita-glue-NOTICE.txt": "3a3b8863de80a922da451c981f84f63ca133959027e38f95e2f4a238f19bc597",
    "tools/vita_licenses/pthread-embedded-COPYING": "933dea73f8be52981372244bea77f4499418f577104b7fc522c1f3f130345352",
    "tools/vita_licenses/pthread-embedded-COPYING.LIB": "a190dc9c8043755d90f8b0a75fa66b9e42d4af4c980bf5ddc633f0124db3cee7",
    "tools/vita_licenses/pthread-embedded-COPYING.vita": "7292d7c97452f761315efe55ba273eb38e0100ebade9ae8fc07fc6f7f05512de",
    "tools/vita_licenses/symphonia-LICENSE": "c76f740d1521b9bed9ca7a04ad526c310493c62621b1341d623b431736533b30",
    "tools/vita_licenses/taiHEN-LICENSE": "b82d83d4c589bfba4c9ce82de4ba98d2b9186b5c59e6cb848bd7ef143b1af437",
    "tools/vita_licenses/vitaGL-COPYING": "3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986",
    "tools/vita_licenses/vitaGL-COPYING.LESSER": "e3a994d82e644b03a792a930f574002658412f62407f5fee083f2555c5f23118",
    "tools/vita_licenses/vitaShaRK-LICENSE": "6557adc4bc1d59232e27567a185dd70cf2ecca3154018b0be61ea29cf9e814e2",
    "tools/vita_release_build.sh": "1014c938120c6069479f1a2b41f0c24efd224970884c29660f48b2134341a2f1",
    # Vita package metadata and original LiveArea art.
    "vita/sce_sys/icon0.png": "87fecbae90a292eedd79da63c552151a41b6ef6e76923fed53a24560de103e6a",
    "vita/sce_sys/livearea/contents/bg.png": "2d2480e5e23fd268a88ed36823837e7ee59afa9b9f193cafa78b24d2f1e13234",
    "vita/sce_sys/livearea/contents/startup.png": "c22f38eaaefd8d2f4e80061045af9588d1e5aa4675f588ed25fc4adfe6bf306a",
    "vita/sce_sys/livearea/contents/template.xml": "c4b64ab985b6b782e240dac3ed0d525e13746bc338d1613fa467e3f371b49081",
    "tools/actor_gpu/base/PL-rc15-release-play.json": "580f73ba3e3bd166fe2362e2e2a9ab229ad2080d0de8f1036b24d80975585c70",
    "tools/actor_gpu/base/PROVENANCE.json": "28a1ab56efeeeaaa24aa84024cdb4e6001fb671cc7a274bf69cade4e04841fc3",
}
LIVEAREA_ART = {
    "vita/sce_sys/icon0.png": (128, 128),
    "vita/sce_sys/livearea/contents/bg.png": (840, 500),
    "vita/sce_sys/livearea/contents/startup.png": (280, 158),
}

# User-requested README footage. Exact files/content only, never a general
# allowance for original game assets or raw research captures.
GAMEPLAY_MEDIA = {
    "docs/media/gameplay/story-preview.gif": "697ceef7f8108cbfd3521c7892b9db5a9a6da8a79eb169aefac346547440db5d",
    "docs/media/gameplay/story-poster.jpg": "51c96dff0690f5908a786cfe2b4eed7453db79fea8c89e9d364a64f47037eeed",
    "docs/media/gameplay/dry-landing.mp4": "e04e476846a5e12b947ffba3616eefd5a8759d82cbd927182c9d0dc3d2da08fe",
    "docs/media/gameplay/dry-landing.gif": "b1fca02df8d1714c007d00932f644c778fafd69d9883475129135cc7049bd6c4",
    "docs/media/gameplay/pool-of-tears.mp4": "5840a570947d79472e9b4087344f06a3d35110fdf1d64f9c29b9826f49f4dc2b",
    "docs/media/gameplay/pool-of-tears.gif": "707bae12ba4cdbbff78d0b4588c717d7ee988b7fd70c2d0c395aa0b22f1cc839",
    "docs/media/gameplay/pale-realm.mp4": "864262da7717b8e193be1f6f6f4f994de90b1781b8c804da62869f678b9102b0",
    "docs/media/gameplay/pale-realm.gif": "56e0ef4b94b96493212ac2fe39369b458960d6c2212c2bf512246e5cad74d5fe",
}
SECRET = re.compile(
    rb"(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}|"
    rb"AKIA[A-Z0-9]{16}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----)"
)
# actor_publication_uv_reuse holds a slot's UVs while the live skeletal mesh array is the same.
# No production code writes a live skeletal UV, and this check keeps it that way: a .uv write in
# any other file fails. These files were audited on 2026-11-03: copies, Prop, loot and world
# meshes, the uniform of that name, the reuse itself and tests. Before adding a file, check
# that the write cannot reach a live skeletal mesh. If it can, the writer must call
# render_fx::actor_publication::note_live_uv_edit (today cfg(test): promote it).
UV_WRITE_FILES = {
    "src/bsp.rs", "src/loot_art.rs", "src/power_pose/effects.rs", "src/render.rs",
    "src/render/vita_renderer.rs", "src/render_fx.rs", "src/render_fx/actor_publication.rs",
    "src/render_fx/prepared.rs", "src/render_fx/retained.rs", "src/render_fx/retained/state_cache.rs",
    "src/character/skinning/overlap.rs", "src/sim_maps/probe.rs", "src/weapons.rs",
    "src/render/vita_renderer/sky_room.rs",
}
UV_WRITE = re.compile(rb"\.uv(?:\.[xy])?\s*(?:[-+*/]?=(?!=))|&mut\s+[\w.\[\]()]*\.uv\b")


def uv_write_errors(path, data):
    if not path.endswith(".rs") or path in UV_WRITE_FILES:
        return []
    lines = [data.count(b"\n", 0, m.start()) + 1 for m in UV_WRITE.finditer(data)]
    return [f"Unaudited .uv write (see UV_WRITE_FILES): {path}:{line}" for line in lines]


def allowed(path):
    p = PurePosixPath(path)
    if p.is_absolute() or ".." in p.parts or "\\" in path:
        return False
    if path in GAMEPLAY_MEDIA or path in SITE_FILES or path in CONFIG_FILES or path in PINNED_FILES:
        return True
    if len(p.parts) == 1:
        return path in ROOT_FILES or (path.startswith("Launch") and p.suffix == ".cmd")
    if p.parts[0] in SUFFIXES:
        return p.suffix in SUFFIXES[p.parts[0]] and "__pycache__" not in p.parts
    return p.parts[:2] in {(".github", "workflows"), (".github", "ISSUE_TEMPLATE")} and p.suffix in {".yml", ".yaml", ".md"}


def size_limit(path):
    return (8 if path in GAMEPLAY_MEDIA and path.endswith(".mp4") else 2) * 1024 * 1024


def livearea_art_errors(path, data):
    """A pinned LiveArea PNG must stay a PNG of the reviewed dimensions."""
    if len(data) < 24 or not data.startswith(b"\x89PNG\r\n\x1a\n") or data[12:16] != b"IHDR":
        return [f"Not a PNG LiveArea asset: {path}"]
    width, height = struct.unpack(">II", data[16:24])
    if (width, height) != LIVEAREA_ART[path]:
        return [f"Unexpected LiveArea art size: {path} is {width}x{height}"]
    return []


def audit_data(path, data):
    errors = []
    if not allowed(path):
        errors.append(f"Not an approved source path: {path}")
    if len(data) > size_limit(path):
        errors.append(f"Unexpectedly large source file: {path}")
    if path in GAMEPLAY_MEDIA:
        if hashlib.sha256(data).hexdigest() != GAMEPLAY_MEDIA[path]:
            errors.append(f"Unreviewed gameplay media content: {path}")
    elif path in PINNED_FILES:
        if hashlib.sha256(data).hexdigest() != PINNED_FILES[path]:
            errors.append(f"Unreviewed pinned source content: {path}")
        if path in LIVEAREA_ART:
            errors.extend(livearea_art_errors(path, data))
        elif b"\0" in data:
            errors.append(f"Binary content in source file: {path}")
    elif b"\0" in data:
        errors.append(f"Binary content in source file: {path}")
    if SECRET.search(data):
        errors.append(f"Possible credential/private key in: {path}")
    errors.extend(uv_write_errors(path, data))
    if path.endswith(".json"):
        try:
            json.loads(data)
        except (ValueError, UnicodeDecodeError):
            errors.append(f"Invalid JSON source data: {path}")
    return errors


def working_files(root=ROOT):
    """Only Git-visible files; include new source, omit deleted and ignored files."""
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root
    ).decode("utf-8").split("\0")
    result = []
    for name in sorted(set(names) - {""}):
        p = root / name
        if not p.exists() and not p.is_symlink():
            continue
        if p.is_symlink() or not p.is_file() or not p.resolve().is_relative_to(root.resolve()):
            raise ValueError(f"Not an ordinary source file: {name}")
        result.append(p)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worktree", action="store_true", help="audit current tracked and new files without staging")
    args = parser.parse_args()
    if args.worktree:
        files = working_files()
        errors = []
        for p in files:
            errors.extend(audit_data(p.relative_to(ROOT).as_posix(), p.read_bytes()))
        if errors:
            print("Source worktree audit failed:\n" + "\n".join(errors))
            return 1
        print(f"Source worktree audit passed: {len(files)} ordinary allowlisted files.")
        print("This does not establish full-history or original-text provenance review.")
        return 0
    index = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=ROOT)
    errors = []
    count = total = 0
    for record in index.split(b"\0"):
        if not record:
            continue
        metadata, raw_path = record.split(b"\t", 1)
        mode, blob, stage = metadata.decode("ascii").split()
        path = raw_path.decode("utf-8")
        count += 1
        if mode not in {"100644", "100755"} or stage != "0" or not allowed(path):
            errors.append(f"Not an approved ordinary source file: {path}")
            continue
        size = int(subprocess.check_output(["git", "cat-file", "-s", blob], cwd=ROOT))
        if size > size_limit(path):
            errors.append(f"Unexpectedly large source file: {path}")
            continue
        data = subprocess.check_output(["git", "cat-file", "blob", blob], cwd=ROOT)
        total += len(data)
        errors.extend(audit_data(path, data))
    if errors:
        print("Source audit failed:\n" + "\n".join(errors))
        return 1
    print(f"Source index audit passed: {count} files, {total:,} bytes; no disallowed paths/binaries or recognised credential patterns.")
    print("This checks the index, not all history, provenance or every possible secret.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
