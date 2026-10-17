#!/usr/bin/env python3
"""Generate unmodified campaign-chain entrances in an isolated evidence directory.

The strict r3f chain may stop early. Remaining upstream visits are generated in
separate processes with explicit chapter loadouts and keep fixture=true. Never
present these as a completed campaign or mix their ledgers into strict proof.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', required=True, type=Path)
    p.add_argument('--data', required=True, type=Path)
    p.add_argument('--evidence', required=True, type=Path)
    p.add_argument('--renderer', required=True, type=Path)
    p.add_argument('--sim', required=True, type=Path)
    args = p.parse_args()
    binary, data, root = args.binary.resolve(), args.data.resolve(), args.evidence.resolve()
    out = root / 'checkpoints'
    out.mkdir(parents=True, exist_ok=True)
    if any(out.iterdir()):
        p.error('checkpoint output must be empty; preserve previous evidence separately')
    env = {k: v for k, v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
    commands = []

    def run(name, flags):
        cell = root / 'host' / name
        private = cell / 'private'
        private.mkdir(parents=True, exist_ok=False)
        shutil.copy2(args.renderer, private / 'vita_renderer.json')
        shutil.copy2(args.sim, private / 'vita_sim.json')
        command = [str(binary), '--data', str(data), '--no-audio', *flags]
        with (cell / 'run.log').open('w') as log:
            result = subprocess.run(command, cwd=cell, env=env, stdout=log, stderr=subprocess.STDOUT)
        commands.append(dict(command=command, cwd=str(cell), exit_code=result.returncode,
                             log_sha256=sha(cell / 'run.log')))
        return cell / 'private' / 'campaign-chain'

    chain = run('strict', ['--campaign-route-check', '--campaign-strict'])
    for f in chain.glob('[0-9][0-9]-*.json'):
        shutil.copy2(f, out / f.name)
    for visit in range(1, 40):
        if list(out.glob(f'{visit:02}-*.json')):
            continue
        chain = run(f'fixture-{visit:02}', ['--campaign-route-check', '--campaign-from',
                                         str(visit), '--campaign-to', str(visit)])
        for f in chain.glob(f'{visit:02}-*.json'):
            shutil.copy2(f, out / f.name)
    rows = []
    for f in sorted(out.glob('*.json')):
        cp = json.loads(f.read_text())
        rows.append(dict(file=f.name, visit=cp['index'], map=cp['entrance']['level']['map'],
                         entry=cp['entrance']['level']['entry'], state_hash=cp['entrance_digest'],
                         sha256=sha(f), strict=cp['strict'], fixture=cp['fixture'],
                         diagnosis=cp.get('diagnosis', False)))
    result = dict(binary_sha256=sha(binary), renderer_sha256=sha(args.renderer),
                  sim_sha256=sha(args.sim), game_archives={f.name: sha(f) for f in sorted(data.glob('*.pk3'))},
                  commands=commands, checkpoints=rows,
                  missing=[i for i in range(1, 40) if i not in {r['visit'] for r in rows}])
    (root / 'MANIFEST.json').write_text(json.dumps(result, indent=2) + '\n')
    (out / 'SHA256SUMS.txt').write_text(''.join(f"{r['sha256']}  {r['file']}\n" for r in rows))
    print(f"Generated {len(rows)}/39; missing: {result['missing']}")


if __name__ == '__main__':
    main()
