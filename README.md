<div align="center">

# Ika Launcher

**Un launcher de Minecraft para Windows, nativo y rápido, con fondos animados de verdad.**

Rust · wgpu · winit · DirectX 12

![Versión](https://img.shields.io/badge/versión-1.6.0--alpha-7c5cff)
![Plataforma](https://img.shields.io/badge/plataforma-Windows%20x64-0078d4)
![Rust](https://img.shields.io/badge/Rust-2024-dea584)

</div>

---

## ¿Qué es Ika?

Ika es un launcher de Minecraft centrado en tres cosas: **arrancar rápido**,
**gestionar instancias con mods sin dolor** y **verse increíble** con fondos de
Wallpaper Engine, vídeos e imágenes detrás de la interfaz.

La versión 1.6 es una **reescritura completa en Rust nativo**. La interfaz se
dibuja directamente en la GPU con `wgpu`, sin navegador embebido, sin HTML y sin
WebView.

## La historia

Ika empezó como **ModComparer**, una pequeña herramienta para comparar los mods
de dos instancias y ver cuáles faltaban. Poco a poco fue sumando piezas:
gestor de instancias, cuentas y skins, fondos animados, amigos, grupos y
compartir instancias. Sin darse cuenta, el comparador se convirtió en un
launcher completo: **Ika Launcher**.

Hasta la versión 1.5.7 Ika usó **Tauri + WebView2**. Funcionaba, pero la
interfaz web tenía un techo de rendimiento: memoria, tiempos de arranque y
fluidez con fondos animados de fondo. Por eso, desde la 1.6.0, Ika se reescribe
sobre **wgpu + winit**:

| | 1.5.x (WebView2) | 1.6+ (wgpu) |
|---|---|---|
| Interfaz | HTML/CSS/JS en WebView2 | Dibujada en GPU, nativa |
| Fondos animados | Capa web sobre el vídeo | Textura compartida en GPU, sin copias a CPU |
| Estado | Legacy, sin nuevas funciones | Desarrollo activo |

> La versión WebView2 se conserva en la rama
> [`legacy-webview2`](https://github.com/Xamply/Ika/tree/legacy-webview2)
> y en el tag [`v1.5.7`](https://github.com/Xamply/Ika/releases/tag/v1.5.7).

## Características

### Minecraft
- **Instancias de Ika, Prism, Pinecone/ElyPrism, Freesm, Battly** y otras
  ubicaciones compatibles, abiertas en sus carpetas originales.
- **Crear y editar instancias**: versión, loader (Forge, NeoForge, Fabric,
  Quilt), Java, RAM, argumentos JVM, portada e icono.
- **Modpacks de CurseForge y Modrinth** desde el propio launcher.
- **Java automático**: si el runtime elegido no está instalado, Ika lo descarga.
- **IkaBoost** para Forge 1.20.1: caché de clases transformadas por Mixin que
  acelera los arranques siguientes.
- Arranque más rápido al desactivar capas Vulkan implícitas (Overwolf, OBS…)
  solo para el proceso del juego.

### Cuentas y social
- Cuentas y skins, con servidor de skins local mediante `authlib-injector`.
- Amigos: enviar, aceptar, rechazar y quitar solicitudes, con identidad del
  dispositivo firmada con Ed25519.

### Personalización
- **Fondos de Wallpaper Engine** (escenas y vídeos del Workshop), vídeos
  locales, imágenes y GIF importados.
- Vídeo con `libmpv` y decodificación por hardware, directo a la textura de la
  GPU.
- Playlists, blur del panel, oscurecimiento y ajustes por escena.
- Reloj flotante, paneles laterales y la colección de instancias en Inicio.

## Migrar desde la 1.5.x

Ika 1.6 **lee los datos de la versión WebView2**: tus instancias, cuentas,
cuenta activa y configuración se reutilizan sin perder nada. Las instancias se
abren en sus carpetas originales y los cambios de la versión nativa se guardan
en `%LOCALAPPDATA%\IkaNative`.

## Estado del proyecto

La 1.6 está en **alpha**. Lo esencial ya funciona, pero aún no tiene todo lo
que tenía la 1.5.7. Pendiente de migrar, en este orden:

- [ ] Gestor de instancias: mods, shaders, resourcepacks, papelera, configs
- [ ] Comparador y escáner de mods
- [ ] Servicios en la nube: nombres, salas, buzón, grupos, tiempo real
- [ ] Compartir instancias con amigos
- [ ] Servidores
- [ ] Actualizador, temas e idiomas

## Compilar

**Requisitos:** Windows 10/11 x64, Rust estable, Python 3.13+, Git y
Visual Studio Build Tools con C++.

```powershell
git clone https://github.com/Xamply/Ika.git
cd Ika
python -m pip install "Pillow>=11.3"
python -B bridge\run.py
```

`run.py` compila lo que haya cambiado y después abre el launcher, como
`npm run tauri dev`. Si la compilación falla, no abre nada. Para vídeo,
`scratch\we-bridge-bin` necesita además `libmpv-2.dll` y `d3dcompiler_43.dll`.

**CurseForge:** la búsqueda de CurseForge necesita una API key propia. Copia
`.env.example` como `ika.build.local` y rellena `IKA_SEED`, o define esa
variable de entorno. `build.rs` la sella en el binario al compilar. Sin ella,
Ika funciona igual pero sin CurseForge.


## Documentación

- [Arquitectura](docs/ARQUITECTURA.md): renderizado, puente de Wallpaper
  Engine, vídeo, mediciones de GPU, IkaBoost y pruebas.

## Agradecimientos

- [wgpu](https://wgpu.rs), [winit](https://github.com/rust-windowing/winit) y
  [egui](https://github.com/emilk/egui).
- [libmpv](https://mpv.io) para el vídeo.
- [Microsoft Detours](https://github.com/microsoft/Detours).
- [authlib-injector](https://github.com/yushijinhun/authlib-injector).
- [Prism Launcher](https://prismlauncher.org) por sus metadatos de versiones.
- Fuentes Minecraftia (CC BY-SA 3.0) y Silkscreen (OFL).

<div align="center">

Hecho por Faally

</div>