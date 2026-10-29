# Renderer du monde Vita — jalons 2/3

Base : `vita-bench-modes` à `8de8940cf17addf73c6cde0b902770bf77f40017`.
Branche : `vita-renderer`. Feature optionnelle `vita_renderer`, désactivée par défaut.
Miniquad reste épinglé à `6f891804`, macroquad à 0.4.14. Pas de changement du lockfile.
Le plan et les points d’étape sont dans le dépôt Alice, branche `feat/retained-plan`.
Jalon 2 figé : `vita-renderer` à 70dcb6c. Jalon 3 séparé : `vita-renderer-j3`,
rebase propre sur single-load 3590678 ; les cinq commits rejoués sont identiques
selon `git range-diff`. Aucun changement de la référence du pilote.

## Chemin et périmètre

`gvillage` utilise les mêmes Assets, BSP, textures, lightmaps et placements LG. Les
autres cartes gardent le renderer précédent. Le nouveau chemin compile au chargement
des groupes par recette, cellule spatiale et modèle parent. Il upload des VBO/IBO
immuables, puis précalcule les listes PVS de tous les clusters, avec listes identiques
partagées. Les anciennes copies CPU de batches et leurs matériaux sont libérées
après cette compilation. Le pic de chargement garde temporairement les deux formats.

La frame teste seulement les bornes des groupes de sa liste PVS. Un parent mobile
reçoit la matrice de `Interactions::transforms`, sans réécriture des sommets. Les
indices ne sont ni filtrés, copiés ni triés pendant la frame. Le cull GPU et les
uniformes remplacent les boucles par sommet de la préparation précédente.

Ordre avec `alpha_interop=true` (jalon 3) : fond atmosphérique, opaque/cutout du
monde, opaque des acteurs, **alpha commun monde/acteurs/objets/effets par groupe**,
première personne après clear de profondeur, puis HUD. Les couches d’un acteur
restent contiguës et dans l’ordre déclaré. Clé : profondeur du centre sur l’axe de
vue ; ordre stable à égalité. Pas de tri/compaction par triangle. Les intersections
au sein d’un groupe restent approximatives, à capturer.

La géométrie animée LG des acteurs est conservée, puis streamée une fois par paquet
entier. Le shader direct reprend exactement les sources LG : apparences, cutout,
blends/facteurs, fog et lumières sélectionnées/models-only. Les uniformes de chaque
paquet sont indépendants. Les buffers stream sont réutilisés ; le backend compact
épinglé retraite leur stockage Vita via glBufferData. Ils appartiennent à Scene et
sont détruits avec elle. Aucun upload ni lecture de géométrie **du monde** dans cette file.

`merge_fx=true` réunit les **meshes dynamiques entiers adjacents** de même état,
au plus 60 000 sommets / 180 000 indices. Une surface du monde ou un acteur multi-pass
fait barrière. L’ordre final est déjà connu : les indices des acteurs sont décalés
une fois, sans trier leurs triangles, puis uploadés. Les copies/indices dynamiques
restent comptés séparément des quatre invariants à zéro du monde. Cette fusion réduit
notamment les draws des puffs adjacents ; elle ne regroupe pas à travers un transparent
du monde. La production/skinning LG peut rester le coût dominant hors monde.

Deux flushs explicites maximum de l’intervalle 3-D complet : `flush_vita_3d_begin`
puis `flush_vita_3d_packets` (file Macroquad résiduelle avant émission directe).
Les scopes de profondeur différée et ghost/dissolve capturent leur politique dans
les paquets ; ils ne déclenchent pas de flush local. Première personne : clear direct,
nouvelle file sans flush supplémentaire. Le masque profondeur est restauré à 1.
Les captures/previews hors banc et le flush final automatique HUD/next_frame restent
extérieurs à cet intervalle ; aucun gain GPU n’est déduit de leur déplacement.

`alpha_interop=false` retrouve le pont du jalon 2 : acteurs opaques Macroquad,
monde alpha avant l’ancienne file des transparents/effets ; paires de flush legacy.
`renderer=legacy` conserve le chemin ancien. Autres cartes : legacy. Monde seul :
la file acteurs n’est pas ouverte et le chemin direct du jalon 2 reste identique.

## Réglages du même VPK

Fichier **lu une fois au démarrage**, hors sauvegardes :
`ux0:data/alice/private/vita_renderer.json` (hôte : `private/vita_renderer.json`).
Le pilote change le fichier, ferme puis relance l'application ; pas de recompilation.

```json
{
  "renderer": "vita",
  "world_only": false,
  "max_layers": 2,
  "sort_alpha": true,
  "sky": true,
  "cell_size": 256.0,
  "alpha_interop": true,
  "actor_max_layers": 2,
  "merge_fx": true,
  "pvs_cache": false,
  "pvs_method": "sources",
  "pvs_verify": false,
  "gpu_experiment": "unchanged"
}
```

- `renderer` : `vita` ou `legacy`, pour A/B dans le même binaire.
- `world_only` : masque acteurs, ramassables, effets/particules et HUD uniquement en
  `renderbench`. Il fonctionne avec les deux renderers. Les 60 frames d'échauffement
  et la simulation pendant cet échauffement restent celles du banc livré.
- `max_layers` : 1 ou 2 contributions par surface ; les stages `detail` sont supprimés.
- `sort_alpha` : tri des groupes, ou ordre PVS stable. Aucun tri par triangle.
- `sky` : fond atmosphérique simplifié activé, ou seul clear atmosphérique.
- `cell_size` : 64 à 2048 unités, modifie les groupes **au chargement**. Une grande
  cellule réduit les draws mais augmente la géométrie/les pixels inutiles.

- `alpha_interop` : scheduler commun on/off, seulement avec gvillage/vita hors monde seul.
- `actor_max_layers` : 1 ou 2 couches d’acteurs/objets ; appliqué au chemin direct,
  avant préparation des surfaces enregistrées. Couches supplémentaires supprimées et comptées.
- `merge_fx` : fusion des paquets dynamiques adjacents de même état on/off ; comparaison
  du coût par draw, sans toucher à la taille des groupes du monde.

- `pvs_method` : `sources` (défaut) ou `legacy`, comparaison du précalcul au chargement.
- `pvs_cache` : cache privé PVS on/off (défaut off jusqu’aux mesures Vita).
- `pvs_verify` : comparaison exhaustive au compilateur précédent ; défaut off.
  Diagnostic coûteux, réservé aux preuves hors mesures ; son temps entre dans load_pvs_ms.

- `gpu_experiment` : `unchanged` (défaut, aucun setter), `gpu111` ou `gpu222`.
  Expérience GPU seule nommée, appliquée une fois au démarrage. ARM/bus/xbar non
  modifiés ; log rc/before/after et quatre horloges réelles du rapport à vérifier.
  Ne pas combiner avec `clk444` ; comparer en renderbench. `gpu111` rétablit la référence
  après un run GPU222. Les horloges peuvent persister entre applications.

Fichier absent : défauts ci-dessus. Champ inconnu, erreur JSON ou valeur hors limites :
erreur explicite dans le log et tous les réglages par défaut. Vérifier le log effectif.
`[lg/vita-renderer] config {...}` imprime la configuration effective au démarrage.

The config record stays one JSON document after its tag: the console auditors `json.loads`
everything after `[lg/vita-renderer] config ` (`tools/config_record/test_auditor_parsers.py`
checks them all against real host lines). The effective report cadence has its own record,
`[lg/vita-renderer] report_cadence effective_log_frame_time_every=<n|off>`.

- `log_frame_time_every`: miniquad timing report cadence in frames. Omitted keeps the
  historical 10, `0` or `null` turns it off. A malformed value drops only this key (logged as
  `invalid config key`) and keeps 10. Measurement configs set 0: in r15b, report=10 added
  3.2/5.0 ms to p95 with no median change. The r12c-era runners count these lines as a hang
  heartbeat (`run-session.py:89-94`), so a default of 0 needs those runners migrated first.
- `sim_trigger_facts_reuse`: `false` (default). Builds trigger facts once per trigger pass and
  rebuilds them after each fire.
`[lg/vita-renderer] loaded {...}` imprime l'inventaire. Le JSON du banc ajoute
`vita_renderer.configuration`, `world_only_active`, `inventory` et `view`.

## Compromis effectifs

- Ciel miniature retiré, fond atmosphérique ; portails caméra remplacés par la
  surface matérielle chargée ; pas de rendu secondaire de scène.
- Deux couches au plus ; détail retiré ; blends AlphaAdd/ColorAdd rapprochés d'Add,
  AlphaColor et facteurs rares rapprochés d'Alpha ; Equal rapproché de LessOrEqual.
- Texture × lightmap et seuils alpha conservés ; pas de light dynamique dans le
  shader du monde de ce jalon. Le chemin des personnages garde ses lumières.
- AnimMap, gain RGB/alpha, UV affines, un terme Turb et une déformation Wave/Move/
  Bulge en shader. Pour plusieurs Turb, seul le dernier terme est représenté ;
  déformations supplémentaires, WaveNormal et bruit sont approchés/figés.
- TAN décoratif, billboards et corde en pose chargée. Les matrices des parents
  interactifs restent actives. Aucun changement des colliders, scripts ou saves.
- PVS par groupe et frustum par AABB : inclusion de toute une surface/groupe,
  donc triangles supplémentaires possibles. L’ordre alpha par groupe
  diffère du tri triangle legacy ; croisements/auto-intersections restent à capturer.

## Chargement — jalon 3

L’observation pilote « environ dix minutes contre cinq » du jalon 2 n’est pas encore
quantifiée. Ce paquet reste intact. Le jalon 3 inclut le single-load 3590678 et publie
`load groups` (recettes, chunks, compteurs), `load pipelines/uploads`, `PVS sources`,
`PVS phases`, `load PVS`, puis `loaded` avec ces temps en inventaire. Ils commencent
après le premier `Loaded gvillage` ; comparer aussi lancement → Loaded → loaded →
caméra gelée. Les 60 frames de warmup ne font pas partie du chargement Scene pur.

Ancien PVS : `Visibility::bounds` parcourt les feuilles pour chaque groupe **et chaque
cluster** ; `faces` reconstruit son masque à chaque cluster. Nouveau : inversion
faces → clusters une fois, intersections AABB → clusters une fois par groupe, union
pour les déformations, puis tests de bits de visibilité. Géométrie hors feuilles et
PVS None restent conservateurs. Gvillage défaut : 163 scans AABB, au lieu de
163 × 2 139, et 8 039 IDs de clusters sources. Hôte : égalité exacte des listes
None + 2 139 clusters contre l’ancien compilateur ; aucun chiffre extrapolé à la Vita.

Cache optionnel : `ux0:data/alice/private/renderer-cache/pvs-<SHA256>.bin` (hôte private).
Clé : version et toutes les entrées de visibilité, groupes/plages, AABB, faces,
opaque/animé/déformé, ordre de pipelines. Format entier u32 little-endian partagé
PC/ARM32, listes internées, checksum du corps, borne 16 Mio, IDs/ranges vérifiés.
Corruption/incompatibilité/absence : reconstruire, jamais croire un PVS tronqué.
Écriture temporaire puis rename ; échec d’I/O : jeu maintenu, erreur explicite.
Cache lu/construit **avant** warmup, sans toucher aux sauvegardes. Aucun GPU ID,
texture ni asset dans ce fichier ; aucun fichier privé dans Git/VPK. Le cache
évite le précalcul PVS, pas le parsing/chargement LG ni les uploads. Le comparer
miss/hit/cache off sur Vita : SD, checksum et allocations peuvent annuler le gain.

Pré-calcul des buffers complets sur PC et chargement incrémental restent possibles,
mais demandent une identité de corpus/recettes portable et une préparation Scene
asynchrone ; les textures et assets LG doivent toujours charger. Ne pas engager cette
refonte avant les temps par phase natifs. Le chargement jusqu’au gameplay et son pic
mémoire comptent dans le jalon 4, au même titre que les performances en jeu.

## Vérification et mesure

Hôte : `cargo check --locked`, `cargo check --locked --features vita_renderer`,
`cargo test --locked --features vita_renderer`. Les tests vérifient layout/uniformes,
UV avec Turb au milieu d'une chaîne, PVS None/A/B/A, limites de chunks/u16,
frustum, masque limité à renderbench, et matrice de motion contre le calcul LG.

Visualiseur sans simulation ni écriture de saves, données personnelles hors Git :

```sh
cargo run --locked --features vita_renderer -- --vita-render-view \
  --data /chemin/personnel/bin/base --map gvillage --frames 6 --capture /tmp/village.png
```

Il emploie la position de référence et la caméra Follow LG à 132 unités ; ce n'est
pas une reproduction certifiée de la pose console. `--start-at "x y z"` choisit un
œil alternatif. Sans `--frames` : WASD et Q/E pour déplacer la caméra ; flèches ou glisser
avec le bouton gauche pour orienter ; Escape ferme.
Captures hors fenêtres de performance. Logs et captures hôte restent des preuves
visuelles/structurelles, sans valeur de temps GPU Vita.

Le contrat du banc ne change pas : bench 120 frames, analyse 31–120 ; renderbench
et fullbench 60 exclues puis 120 mesurées, 120 `frame_wall_ms`, zéro save/preview.
Budget de 25 ms : **médiane `frame_wall_ms` en renderbench + monde seul**, avec
configuration/caméra/horloges/résolution publiées. Les spans CPU ne décident pas.
Les compteurs `vita_world_gpu_draws`, `vita_world_*_triangles`, changements de
pipeline/texture et octets d'uniformes comptent les appels directs émis par LG ;
ils ne constituent pas un compteur interne de vitaGL ni une mesure de surtirage.
Les spans `vita_cluster_frustum`, `vita_world_parameters`, `vita_world_submit`
et `vita_alpha_group_sort` sont séparés ; `vita_world_total` est leur parent,
avec les frontières et le ciel. En interop, la préparation alpha appartient à ce parent,
mais son émission différée est `vita_world_alpha_submit`, enfant de `vita_joint_alpha_submit` ;
les analyser séparément, sans prétendre qu’un seul span continu couvre tout le monde. Les uniformes utilisent un scratch préalloué ;
les quatre compteurs upload/retransformation/indices/triangles triés restent à zéro.
`gpu_geometry_bytes` est le payload uploadé, pas la mémoire physique de vitaGL.

Jalon 3 : `--vita-alpha-check` sur hôte vérifie les vrais shaders monde/acteurs,
l’ordre commun indépendant de l’ordre de soumission, occultation opaque,
clear première personne, couches multiplicatives/low-alpha, ghost/dissolve et lumières.
Les fixtures et captures sont synthétiques et hors banc. `vita_fx_gpu_draws`, uploads,
meshes fusionnés, sommets/indices ajoutés et capacité stream sont hors monde ;
`vita_joint_alpha_groups` mesure les entrées après fusion. Les spans sont séparés
(upload, fusion, tri commun, submit opaque/alpha) sans instrumentation par sommet.

Les sondes legacy à ee297f8 motivent une couche, moins de draws et moins de surtirage.
Aucune hausse d’horloges ni baisse de résolution par défaut : gpu_experiment=gpu222
est une expérience distincte, nommée et pilotée, dans le même VPK. Les timings hôte ne prédisent pas ceux de Vita.

Console/FTP réservés au pilote. Le paquet de jalon contient ELF, VPK, SHA-256,
révisions, vérification imports/heap/piles/marge ELF, réglages et grille de runs.
Le passage jouable, rencontre et save/Continue sont le jalon 4, pas cette livraison.
