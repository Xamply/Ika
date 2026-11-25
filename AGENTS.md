# Instrucciones para agentes de IA (Ika Launcher 1.6, wgpu)

Este archivo es obligatorio para cualquier IA que trabaje en este repo.

## Cómo se abre el launcher (regla principal)

El launcher **siempre** se abre así, desde la raíz del repo:

```powershell
python -B bridge/run.py
```

`run.py` compila antes de abrir (como `npm run tauri dev`): muestra la barra de
`cargo`, recompila el puente de vídeo solo si cambió, y si la compilación falla
no abre nada.

- **No** abras el `.exe` directamente ni compiles en otro `--target-dir`:
  `run.py` solo usa `target\release` y `scratch\we-bridge-bin`, así que tu
  cambio no se vería.
- Si sale "Acceso denegado", el launcher está abierto: pide al usuario que lo
  cierre. No lo esquives con otra carpeta.

## Antes de dar un cambio por terminado

1. `cargo test --release --locked` pasa.
2. Abre la app con `python -B bridge/run.py` y comprueba el flujo que tocaste.
   Si no puedes verlo (por ejemplo, sin acceso a la pantalla), dilo
   claramente y explica al usuario qué debe probar; no lo des por hecho.
3. Informa de lo que no pudiste comprobar.

## Límites

- No hagas `git push`, tags ni releases sin confirmación explícita del usuario.
- No subas a git: `ika.build.local` (credenciales), `media/`, `scratch/`, `target/`.
- No cambies los formatos de datos en disco ni el contrato con el servicio de
  Cloudflare (`profile/social.rs`): los usuarios de la 1.5.x migran con sus datos.
- La versión WebView2 está en la rama `legacy-webview2`. Es solo referencia:
  no la modifiques.
- Detalles técnicos: `docs/ARQUITECTURA.md`.