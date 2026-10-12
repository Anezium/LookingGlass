# Renderer du monde Vita — jalon 2

Base : `vita-bench-modes` à `8de8940cf17addf73c6cde0b902770bf77f40017`.
Branche : `vita-renderer`. Feature optionnelle `vita_renderer`, désactivée par défaut.
Miniquad reste épinglé à `6f891804`, macroquad à 0.4.14. Pas de changement du lockfile.
Le plan et le point d'étape sont dans le dépôt Alice, branche `feat/retained-plan`.

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

Ordre : fond atmosphérique (un triangle), opaque/cutout, acteurs/objets legacy,
alpha du monde trié par distance des centres de groupes, puis effets legacy/HUD.
Le monde a 16 pipelines directs fixes : cinq classes de blend × trois culls, plus ciel.
Le masque d'écriture de profondeur est global par phase et restauré ensuite.
Le monde direct n'utilise aucune paire `flush_depth_begin/end` ni `render_fx::pass`.
Avec acteurs présents, deux frontières d'interop macroquad sont comptées ; les flushs
propres aux acteurs/effets legacy restent au jalon 2. Avec le diagnostic monde seul,
la queue d'effets legacy n'est pas ouverte.

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
  "cell_size": 256.0
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

Fichier absent : défauts ci-dessus. Champ inconnu, erreur JSON ou valeur hors limites :
erreur explicite dans le log et tous les réglages par défaut. Vérifier le log effectif.
`[lg/vita-renderer] config {...}` imprime la configuration effective au démarrage.
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
  donc triangles supplémentaires possibles. L'ordre alpha monde/acteurs peut
  différer du tri global legacy : cas à capturer et revoir au jalon 3.

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
œil alternatif. Sans `--frames` : WASD et Q/E pour déplacer la caméra ; Escape ferme.
Captures hors fenêtres de performance. Logs et captures hôte restent des preuves
visuelles/structurelles, sans valeur de temps GPU Vita.

Le contrat du banc ne change pas : bench 120 frames, analyse 31–120 ; renderbench
et fullbench 60 exclues puis 120 mesurées, 120 `frame_wall_ms`, zéro save/preview.
Budget de 25 ms : **médiane `frame_wall_ms` en renderbench + monde seul**, avec
configuration/caméra/horloges/résolution publiées. Les spans CPU ne décident pas.
Les compteurs `vita_world_gpu_draws`, `vita_world_*_triangles`, changements de
pipeline/texture et octets d'uniformes comptent les appels directs émis par LG ;
ils ne constituent pas un compteur interne de vitaGL ni une mesure de surtirage.
`gpu_geometry_bytes` est le payload uploadé, pas la mémoire physique de vitaGL.

Console/FTP réservés au pilote. Le paquet de jalon contient ELF, VPK, SHA-256,
révisions, vérification imports/heap/piles/marge ELF, réglages et grille de runs.
Le passage jouable, rencontre et save/Continue sont le jalon 4, pas cette livraison.
