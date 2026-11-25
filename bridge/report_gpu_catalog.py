"""Turn a verify_gpu_catalog.py results.json file into a ranked Markdown report."""

import argparse
import json
from pathlib import Path
from statistics import median


def metric(run, key, stat='median'):
    return (run.get('metrics', {}).get(key) or {}).get(stat)


def fmt(value, decimals=1):
    return '—' if value is None else f'{value:.{decimals}f}'


def pair(run, key):
    return f'{fmt(metric(run, key))} / {fmt(metric(run, key, "max"))}' if 'metrics' in run else '—'


def md(value):
    return str(value).replace('|', r'\|').replace('\n', ' ')


def resolution(px):
    return '×'.join(map(str, px)) if px else '—'


def render(data):
    items = data['items']
    n = sum(item['playable'] for item in items)
    good, missing, comparable = [], [], []
    for item in items:
        runs = {run['mode']: run for run in item['runs']}
        active = runs.get('direct_alias' if item['kind'] == 'scene' else 'media', {})
        ref = runs.get('copy', {})
        if 'metrics' in active:
            good.append((item, active, ref))
        if not item['playable']:
            missing.append((item, 'Sin reproducción integrada'))
        elif 'metrics' not in active:
            missing.append((item, active.get('error', 'Pendiente')))
        elif item['kind'] == 'scene' and 'metrics' not in ref:
            missing.append((item, 'Control copy: ' + ref.get('error', 'pendiente')))
        if ('metrics' in active and 'metrics' in ref
                and active['source_px'] == ref['source_px'] and active['window_px'] == ref['window_px']
                and active['foreground_samples'] == ref['foreground_samples'] == data['sample_count']
                and abs(active['viewer_fps']['median'] - ref['viewer_fps']['median']) <= 2
                and active['source_fps_estimate'] is not None and ref['source_fps_estimate'] is not None
                and abs(active['source_fps_estimate'] - ref['source_fps_estimate']) <= 2):
            comparable.append((active, ref))

    lines = [
        '# GPU por fondo del catálogo',
        '',
        f'**{len(good)}/{n} reproducibles medidos** de {len(items)} entradas locales. '
        f'GPU: {data["adapter"]}. Estado: {"terminado" if data["complete"] else "en curso"}.',
        '',
        'Un visor y un productor por corrida, Inicio maximizado, audio silenciado, sin precarga del catálogo. '
        f'Escenas a resolución nativa y objetivo de {data["scene_target_fps"]} FPS; '
        'vídeos a resolución y cadencia de archivo. '
        f'{data["sample_count"]} muestras, una por segundo, tras arranque y estabilización. '
        'Los máximos pertenecen a esa ventana corta, no a toda la reproducción.',
        '',
        'GPU, decodificador y W son contadores de la **tarjeta completa**. '
        'Visor 3D, productor 3D y RAM privada residente se miden por proceso. '
        'La cifra por proceso depende de la atribución de Windows; no se deben sumar motores. '
        'FPS de fuente se estima a partir de bloques de 60 cuadros; en una imagen fija o un vídeo lento puede faltar.',
        '',
        '[Resultados crudos, muestras, FPS y memoria por proceso](results.json).',
        '',
    ]
    for label, key in (('Antes', 'desktop_before'), ('Después', 'desktop_after')):
        if values := data.get(key):
            lines.append(f'- {label}, escritorio: {fmt(median(x["gpu_pct"] for x in values))}% GPU, '
                         f'{fmt(median(x["watts"] for x in values), 2)} W. '
                         'El valor de escritorio incluye otras apps.')
    if comparable:
        p = len(comparable)
        lines.extend([
            '',
            f'**{p} pares comparables** en tamaño, foco de ventana y cadencia (±2 FPS). '
            'Cambios medianos por fondo de `copy` a `direct_alias`:',
            '',
        ])
        for key, label, unit in (
            ('gpu_pct', 'GPU total', ' pp'), ('watts', 'Potencia total', ' W'),
            ('producer_3D', '3D del productor según Windows', ' pp'),
            ('viewer_private_resident_mib', 'RAM privada residente del visor', ' MiB'),
            ('producer_private_resident_mib', 'RAM privada residente del productor', ' MiB'),
        ):
            deltas = [metric(active, key) - metric(ref, key) for active, ref in comparable
                      if metric(active, key) is not None and metric(ref, key) is not None]
            if deltas:
                lines.append(f'- {label}: {median(deltas):+.2f}{unit}.')

    if good:
        lines.extend([
            '', '## Carga por tipo de fondo', '',
            'Medianas por fondo. Rango de GPU: de menor a mayor mediana entre fondos del tipo.', '',
            '| Tipo | Fondos | GPU total mediana | Rango GPU total | W medianos |',
            '|---|---:|---:|---:|---:|',
        ])
        for kind in ('scene', 'video', 'image'):
            subset = [active for item, active, _ in good if item['kind'] == kind]
            gpu = [metric(run, 'gpu_pct') for run in subset if metric(run, 'gpu_pct') is not None]
            watts = [metric(run, 'watts') for run in subset if metric(run, 'watts') is not None]
            if gpu and watts:
                lines.append(f'| {kind} | {len(subset)} | {fmt(median(gpu))}% | '
                             f'{fmt(min(gpu))}–{fmt(max(gpu))}% | {fmt(median(watts), 2)} |')

    lines.extend([
        '', '## Todos los fondos medidos',
        '',
        'Ordenados por GPU total mediana. «Normal» = `direct_alias` para escenas, libmpv D3D11 para vídeos y WIC para imágenes. '
        'Las columnas con dos cifras muestran mediana / máximo. FPS muestra visor / fuente.',
        '',
        '| Fondo | Tipo | Fuente | FPS | GPU normal % | W normal | Visor 3D % | Productor 3D % | Decodificador % | GPU copy % | W copy | Foco |',
        '|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|',
    ])
    for item, active, ref in sorted(good, key=lambda row: (
            -(metric(row[1], 'gpu_pct') or 0), -(metric(row[1], 'watts') or 0), row[0]['name'])):
        cols = (
            f'{md(item["name"])} (`{md(item["id"])}`)', item['kind'], resolution(active.get('source_px')),
            f'{fmt(active["viewer_fps"]["median"])} / {fmt(active["source_fps_estimate"])}',
            pair(active, 'gpu_pct'), pair(active, 'watts'),
            fmt(metric(active, 'viewer_3D')), fmt(metric(active, 'producer_3D')),
            fmt(metric(active, 'decoder_pct')), pair(ref, 'gpu_pct'), pair(ref, 'watts'),
            f'{active["foreground_samples"]}/{data["sample_count"]}'
            + (f' · {ref["foreground_samples"]}/{data["sample_count"]}' if 'metrics' in ref else ''),
        )
        lines.append('| ' + ' | '.join(cols) + ' |')
    if missing:
        lines.extend(['', '## Entradas sin medición o sin control', '',
                      '| Fondo | Tipo | Estado |', '|---|---|---|'])
        lines.extend(f'| {md(item["name"])} | {item["kind"]} | {md(reason)} |' for item, reason in missing)
    return '\n'.join(lines) + '\n'


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('results', type=Path)
    args = p.parse_args()
    data = json.loads(args.results.read_text(encoding='utf-8'))
    args.results.with_name('informe.md').write_text(render(data), encoding='utf-8')


if __name__ == '__main__':
    main()
