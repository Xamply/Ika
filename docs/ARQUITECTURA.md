# Arquitectura de Ika Launcher 1.6

Notas técnicas del renderizado, los fondos y el lanzamiento de Minecraft.
Para una visión general, consulta el [README](../README.md).

## Wallpaper Engine y MP4 en una ventana `wgpu`

El motor oficial de Wallpaper Engine reproduce los paquetes de escenas instalados.
Una extensión de diagnóstico cargada mediante Microsoft Detours toma el
backbuffer D3D11 de esa instancia justo antes de `Present`. Copia el cuadro
**una vez en la GPU** a una textura DXGI que puede compartir entre procesos y
omite la presentación de ese swapchain. No usa captura de pantalla o ventana,
una segunda reproducción de la escena ni un underlay.

`wgpu` importa esa textura en DirectX 12 y la muestrea directamente. Dos
señales de GPU coordinan la escritura y la lectura, sin descargar píxeles a CPU
ni subirlos de vuelta. `wgpu` dibuja Chisa, el panel y el botón en una pasada.
La única salida en un monitor es la ventana `winit`. El motor conserva un
HWND técnico fuera de los monitores para inicializar su swapchain.

La barra de título del launcher también se dibuja en esa pasada. Usa
la geometría de 52/102 px, un módulo centrado de 940 px y los dos gradientes
de la referencia, animados con el reloj de la interfaz a 120 Hz. Los iconos, textos y
colores se empaquetan en un solo atlas. El visor no carga HTML, CSS ni
un WebView. Las cinco pestañas animan la selección. Algunas pantallas y el servicio
de notificaciones de la versión WebView2 todavía se están migrando.
El botón cuadrado abre el selector de monitor con encendido y apagado CRT.

En Windows, el launcher recupera las cuentas y la cuenta activa de Ika WebView2
del perfil de este usuario. Lee una copia temporal de Local Storage e IndexedDB,
de modo que se puede abrir junto al launcher anterior. Los cambios hechos en la
versión nativa se guardan en `%LOCALAPPDATA%\IkaNative`.

- La tarjeta de usuario abre el selector de cuentas y las skins de ese perfil.
  Arrastra la skin para girarla. La textura elegida y su modelo Steve o Alex
  se pasan a Minecraft con el mismo servidor local de skins y `authlib-injector`
  que usaba la versión WebView2.
- **Preferencias** recupera la portada y el perfil guardados, con las secciones,
  la búsqueda y la navegación del diseño WebView2. Las portadas de vídeo usan el
  puente de libmpv mientras la ventana está visible. Los cambios de cuenta, avatar,
  encuadre y carpeta de instancias se guardan en la versión nativa.
- **Amigos** recupera la lista y usa la identidad guardada del dispositivo para
  enviar, aceptar, rechazar o cancelar solicitudes y quitar amistades. Las
  cuentas locales nuevas necesitan registrarse en la versión WebView2 para usar
  el servicio. La transferencia y comparación de instancias entre amigos
  todavía requieren WebView2.
- **Crear instancia** y **Editar instancia** recuperan el diseño de dos
  columnas del launcher anterior. Permiten guardar instalaciones personalizadas,
  elegir portada e icono y ajustar la versión, el loader, Java, RAM y argumentos
  JVM. Para modpacks, se abre el catálogo oficial del proveedor.
- **Exportar** recupera la ventana del dock y permite guardar los archivos de la
  instancia como ZIP local.

`node tools/profile-reference.cjs RUTA_LEGACY` permite regenerar los iconos
y el arte de estas ventanas desde un checkout de la rama `legacy-webview2`. Las capturas de
referencia se guardan en `scratch/profile-reference`.

Para vídeos, el puente usa libmpv con render D3D11 y decodificación
`d3d11va-zero-copy`: mpv escribe directamente en la textura compartida D3D11
→ DX12, sin `TransferVideoFrame` ni una copia 3D adicional de Wallpaper
Engine. El visor conserva en la GPU el último fotograma completo del vídeo
antes de devolver la textura compartida a mpv. Así, un redibujado de Inicio
entre dos fotogramas no puede leer una imagen a medio escribir.
El productor publica los fotogramas a la resolución y cadencia del
archivo original. El visor aplica un encuadre de tipo `cover` al presentar en
la ventana. Cada fondo con audio se inicia silenciado.
Puedes pulsar
**Activar sonido** en Wallpapers o **M** en la vista principal. El fundido de
bucle se puede activar para diagnóstico con `IKA_VIDEO_LOOP_FADE=1`; queda
apagado en reproducción normal para no reservar una segunda textura 4K.
Al abrir Wallpapers se prepara el siguiente fondo durante la animación de
entrada. Al pasar el cursor por otro fondo local o por **CAMBIAR FONDO**, el
launcher cambia ese candidato. Lo deja en pausa tras el primer cuadro
y lo descarta después de unos segundos si no se usa. Al cambiar de fondo,
el visor conserva el último cuadro del fondo anterior hasta que llega el
nuevo. El cierre del productor anterior ocurre en segundo plano para que no
bloquee la presentación. La transición se ve desde el primer cuadro y dura
110 ms.
Cada fondo mantiene su propio encuadre. La transición reutiliza las
texturas compartidas y se omite si Windows tiene desactivadas las animaciones.
La ventana, la superficie, el dispositivo y el estado del blur se conservan.

La CLI de Wallpaper Engine no expone un destino de render compartido. El
puente intercepta la presentación solo en una instancia de prueba iniciada en
modo de vista previa del salvapantallas, con configuración temporal. No carga
el puente en el Wallpaper Engine ya abierto, ni modifica la instalación o el
paquete del Workshop. Al cerrar la ventana, el lanzador cierra esa instancia.
Esta ruta es experimental: requiere Windows x64 con D3D11/D3D12, SDR de 8 bits y ambos
procesos en la misma GPU; no es una API de Wallpaper Engine. Una actualización
del motor o de DirectX puede requerir revisar la interceptación. No se probó
HDR ni reenvío de puntero a escenas interactivas.

Esta carpeta contiene el código fuente y un fondo fijo integrado. Para vídeos,
la carpeta de binarios debe incluir `libmpv-2.dll` junto a `video-mpv.exe`.
`bridge\\run.py` selecciona ese ejecutable para vídeos y conserva `video.exe`
como respaldo diagnóstico. Los headers de libmpv están en
`bridge/mpv/include`. Para compilar
en Windows x64 necesitas Rust, Python 3.13+, Git y Visual Studio Build Tools
con C++.

Abre **x64 Native Tools Command Prompt for VS** en esta carpeta y ejecuta:

```bat
git clone --depth 1 --branch v4.0.1 https://github.com/microsoft/Detours.git scratch\we-detours
bridge\build.cmd scratch\we-detours scratch\we-bridge-bin
cargo build --release --locked
python -m pip install "Pillow>=11.3"
python -B bridge\run.py
```

El lanzador detecta las bibliotecas de Steam, las descargas de Workshop en `media/` y los fondos de
`%USERPROFILE%\Downloads\ModComparer_Wallpapers`, incluidos los importados.
No duplica los proyectos presentes en varias bibliotecas. También acepta
proyectos en `media/` o rutas dadas por `--chisa`, `--scene` y `--mp4`.
Wallpaper Engine solo hace falta para reproducir las escenas del Workshop.
Sin él, el lanzador puede usar vídeos locales, MotionBGs, vídeos del Workshop
ya descargados e imágenes. Si no encuentra un fondo compatible, arranca con
**Fondo de Ika**. Las escenas instaladas se pueden consultar en el catálogo,
pero quedan fuera de la rotación y requieren el motor para reproducirse.

Pulsa **Wallpapers** en la barra inferior para abrir el selector y aplicar
un fondo de la biblioteca, buscar por nombre o etiqueta, importar un archivo
o enlace HTTPS directo, armar una playlist o ajustar el oscurecimiento.
En **Fondos Descargados**, el icono de la carpeta abre en el Explorador la
ubicación del fondo que está aplicado.
Las pestañas **Wallpaper Engine** y **MotionBGs** consultan el catálogo al
abrirse y ofrecen búsqueda, filtros y navegación por páginas. El Workshop
aplica los fondos ya instalados; para suscribirte a los demás abre su ficha
en Steam. MotionBGs descarga y aplica el video de mayor resolución disponible.
El engranaje de una escena permite cambiar interruptores, valores numéricos
y opciones. Se guardan el fondo, la playlist y esos ajustes en
`%LOCALAPPDATA%\IkaNative\wallpapers`.
**CAMBIAR FONDO** o **N** recorren los fondos compatibles. **BLUR OFF / ON**
o **Espacio** alterna el desenfoque del panel.
**Esc** cierra primero el selector y, desde la vista principal, el visor.
Usa `--scene` para
añadir carpetas de escenas de Workshop en lugar de Lynae y `--mp4` para
elegir vídeos locales en lugar del incluido en la lista; ambas opciones se
pueden repetir. `--fixtures` recupera los tres fondos de prueba.
`--engine` apunta a otra instalación de Wallpaper Engine,
`--chisa` cambia la primera escena y `--single` mantiene solo esa escena.
Los fondos web se listan y abren en su ficha de Steam. La reproducción de
archivos locales depende de los códecs de Windows; las imágenes, incluidos
los GIF importados, se muestran como fotogramas fijos. Los enlaces deben
apuntar al archivo de imagen o video.

El visor prepara DirectX con la ventana oculta. El primer cuadro visible
muestra la interfaz o el cubo de carga si el fondo aún se está preparando.
El puente publica el fondo inicial sin esperar al resto del catálogo. Al
seleccionar otro fondo, el visor puede mostrar su primer fotograma guardado
mientras el productor prepara la animación. Si Windows tiene desactivadas las
animaciones, el cubo queda fijo y se pasa directamente a la interfaz.
La compilación `--release` trae los shaders FXC precompilados.
Usa `python -B bridge/run.py --fullscreen` para ocupar todo el monitor
principal sin esos controles. Las animaciones de la interfaz y el fondo tienen
cadencias independientes. Las escenas de Wallpaper Engine usan 60 FPS por
defecto. Las imágenes fijas se publican una sola vez. Al minimizar, el visor
deja de dibujar y el productor espera hasta restaurar la ventana.

El modo de vídeo lee el archivo original sin crear copias ni requerir FFmpeg.
En Windows, DirectComposition puede presentar el vídeo y la interfaz como dos
capas de una misma ventana. El vídeo publica cada fotograma; la interfaz
reutiliza su capa entre cambios. El encuadre se aplica al presentar, sin
reescalar la textura que entrega el reproductor. Cuando un efecto necesita
mezclar ambas capas o DirectComposition no está disponible, wgpu compone
la imagen en una sola superficie. El uso de GPU depende del equipo, la
resolución de la ventana y el vídeo.

En Inicio, las franjas donde solo se ve el fondo usan una muestra de textura por
píxel. La interfaz se compone directamente en el espacio de color del fondo SDR,
sin textura intermedia. Los paneles, el oscurecimiento y las transiciones conservan
su tratamiento de color.

Acerca el cursor a cualquiera de los bordes laterales de **Inicio** para
desplegar su panel. A la izquierda están la actividad y la colección de
instancias; a la derecha, los accesos rápidos y las novedades. Los paneles
terminan en el borde superior de la barra inferior. Haz clic fuera del panel
o aléjate del borde para cerrarlo.

**Mi Colección** muestra las instancias en cinco columnas, con pestañas,
búsqueda por nombre y filtros de versión, loader y Java. El borde iluminado
marca la instancia seleccionada y su ruta aparece debajo de la cuadrícula.
Usa el menú de clic derecho de un mosaico para añadirlo a Favoritos o abrir
su carpeta.

Puedes mover el reloj flotante, cambiar su tamaño desde la esquina inferior
derecha o con **Ctrl + rueda**, y soltarlo en un panel lateral para anclarlo.
Su menú de clic derecho cambia el estilo, alterna entre 12 y 24 horas y restaura
el tamaño. La primera vez se recupera la configuración de WebView2 si está
disponible. Los cambios se guardan en `%LOCALAPPDATA%\IkaNative\home-clock.json`.

Al pasar el cursor por una tarjeta, la precarga espera una pausa breve; aplicar
un fondo con clic no espera esa pausa. Si Python se cierra de golpe, el
supervisor de cada escena detiene su productor. Para comparar, añade `--gpu-check-renderer` a
`bridge/verify_gpu.py`: mide en microsegundos la pasada de Inicio y la de
referencia en cuadros consecutivos. El informe distingue cuadros presentados
por el visor de cuadros nuevos recibidos de la fuente.

La barra inferior recrea el diseño de la versión WebView2. WGSL anima los bordes,
la órbita de **Jugar** y la apertura y cierre CRT de **Wallpapers** y del
selector de monitor. La apertura arranca en el primer fotograma presentado:
punto, línea horizontal, destello y despliegue del panel en 760 ms.
El apagado lleva 420 ms. El fósforo se compone aparte del panel, con las
texturas de punto y haz tomadas de la versión WebView2. Como en la referencia,
Wallpapers deja visible el launcher alrededor del panel. El
selector reutiliza la textura del fondo activo y carga solo las miniaturas
visibles. Guarda las imágenes de 464 × 320 en PNG sin pérdidas y libera su
textura al cerrar el panel. Al hacer scroll conserva las miniaturas que siguen
visibles y solo carga las que entran. Si falta una portada local o es pequeña,
usa FFmpeg (si está instalado) para extraer un fotograma del vídeo original.
La geometría y los iconos fijos se guardan en un atlas con
`node tools/dock-reference.cjs RUTA_LEGACY` y `python tools/pack_dock.py`; el visor no
ejecuta un navegador. Los enlaces sociales funcionan; las acciones de
cuenta e instancias son visuales en este visor.

El visor solicita bloques de memoria más pequeños a `wgpu` y usa dos
imágenes de presentación en DX12. En ejecución
normal del selector también pide a Windows recortar páginas inactivas del
visor cuando termina el arranque y, si el uso sigue elevado, después de
volver a Inicio o perder el foco. Esto no reduce por sí mismo la memoria
comprometida, ni fija un techo de RAM. Usa `--no-trim` para medir sin ese
recorte. Los productores y el lanzador tienen sus propias cifras.

El visor usa FXC por defecto para compilar WGSL en DirectX 12. En las pruebas
con una RTX 4070 SUPER, FXC redujo el tiempo de composición frente a DXC
sin bajar la resolución del fondo ni la cadencia. `WGPU_DX12_COMPILER`
permite cambiar de compilador para comparar. Ejecuta
`python -B bridge/verify_gpu.py --gpu-check-catalog --chisa RUTA_DE_ESCENA`
para medir GPU 3D por proceso, FPS estimados y RAM privada residente con
el visor en primer plano, en Inicio y Wallpapers. `--gpu-check-panels` también
mide blur en las vistas con la herramienta de pruebas visible. El resultado queda
en `scratch/we-wgpu-bridge-*/gpu-proof.json`.
Agrega `--gpu-check-minimize` para medir también la pausa y la reanudación,
y `--gpu-check-power` para registrar la potencia y la utilización de la
tarjeta NVIDIA completa por separado de los contadores de Windows.

Para medir todo el catálogo instalado, usa una carpeta de resultados nueva:

```powershell
python -X utf8 -B bridge/verify_gpu_catalog.py --output scratch/gpu-catalog-prueba
python -X utf8 -B bridge/report_gpu_catalog.py scratch/gpu-catalog-prueba/results.json
```

Esta prueba abre un fondo por vez con Inicio maximizado. Compara las escenas
con `copy` y `direct_alias`; mide vídeos e imágenes una vez porque usan la
misma ruta en ambos modos. Guarda muestras crudas en `results.json` y una
tabla ordenada por GPU total en `informe.md`. La prueba usa una sesión temporal
y lee los ajustes de escena actuales.

La transferencia aislada admite varias arquitecturas sin cambiar FPS ni
resolución. El modo normal ahora es `direct_alias`: el motor dibuja en una
textura compartida y el visor DX12 la muestrea directamente. `copy` queda como
respaldo compatible y copia una vez por cuadro. Para comparar usa la misma
prueba y cambia solo la variable de entorno:

```powershell
$env:IKA_TRANSFER_ARCH = 'copy'
python -B bridge/verify_gpu.py --gpu-check-power --fixtures --bin scratch/we-bridge-transfer-bin
$env:IKA_TRANSFER_ARCH = 'direct'
python -B bridge/verify_gpu.py --gpu-check-power --fixtures --bin scratch/we-bridge-transfer-bin
$env:IKA_TRANSFER_ARCH = 'direct_alias'
python -B bridge/verify_gpu.py --gpu-check-power --fixtures --bin scratch/we-bridge-transfer-bin
```

`direct_alias` prueba otra ruta: intercepta `GetBuffer(0)` y entrega al motor
una textura compartida con el mismo tamaño y formato. El motor dibuja
directamente allí; el visor DX12 la abre y el puente solo publica la valla. No
hay `CopyResource` por cuadro.

También se pueden aislar `copy_read` (textura compartida solo como shader
resource), `copy_region`/`copy_region_read` (copia por región completa),
`copy_nt` (handle NT) y `copy_noflush` (omite `Flush` después de la señal).
Todos conservan el mismo tamaño de cuadro y son experimentales; no cambian la
ruta normal.

`direct` exige que Wallpaper Engine exponga su backbuffer como recurso DXGI
compartible. Si el controlador o el motor lo rechazan, la prueba termina con
`Direct frame bridge unavailable` y no cambia silenciosamente a `copy`; así no
se confunden dos mediciones de la misma arquitectura. En la RTX 4070 SUPER de
prueba, el backbuffer oficial devolvió `0x80070057` (recurso no compartible),
por lo que `copy` sigue siendo la ruta válida y no se puede eliminar esa copia
sin modificar el motor productor.

Se eligió `direct_alias` por estas mediciones aisladas en RTX 4070 SUPER:

- `copy`: Chisa llegó a `19.55%` de GPU 3D del productor, `119.9 FPS`,
  `2560x1440`, RAM del visor `8.10/8.13 MiB` (mediana/máximo).
- `copy_read`: funcionó y mantuvo `119.5–120.1 FPS` y la misma resolución,
  pero el productor osciló entre `20.09%` y `24.12%` en repeticiones; no hubo
  reducción estable de potencia o GPU.
- `direct_alias`: Chisa midió `0.20%` y Lynae `0.27%` de GPU 3D del
  productor, `119.9–120.0 FPS`, RAM `8.02/8.14 MiB`. La prueba visual de 10 s
  entregó `1200` cuadros sin cambios fuera de panel/botón.

`direct_alias` conserva una sola textura compartida, igual que `copy`; elimina
la copia completa por cuadro. `copy` y `copy_read` permanecen disponibles para
diagnóstico o compatibilidad.

Para MP4, la prueba equivalente con libmpv D3D11 publicó Iuno 4K como
`2560×1440` y Chainsaw como `1920×1080`, mantuvo la cadencia nativa del vídeo y dejó el
motor 3D del productor en `0.05–0.06%`; el trabajo restante aparece en
`VideoDecode`. El productor MPV también quedó por debajo de la RAM privada de
la ruta Media Foundation en estas corridas.

El visor no vuelve a forzar 120 presentaciones por segundo para un vídeo. Lee
la valla de la textura compartida y presenta a la cadencia publicada por MPV:
Iuno y Chainsaw entregan aproximadamente `60 FPS`, mientras Frogs entrega
`24 FPS`. Cuando no hay un cuadro nuevo, deja de ejecutar el compositor. Para
esos cuadros usa además un shader de una sola muestra en color codificado y
dibuja la interfaz como overlay; en las pruebas el 3D del visor bajó de
`49.6%` a `20.8%` con Iuno, sin cambiar resolución ni la cadencia del archivo.

También queda guardada una prueba experimental de presentación por copia GPU.
`--present-copy --fullscreen` copia directamente una textura compartida cuando
fuente y monitor tienen exactamente el mismo tamaño y formato, y dibuja la
interfaz en regiones separadas. No se activa por defecto: el launcher normal
usa una altura de cliente distinta, muchos vídeos tienen otra resolución y un
vídeo escalado necesita un compositor; forzarlo ahí rompería la imagen o no
reduciría el 3D. Se midió a `120 FPS` y resolución nativa, pero la mejora solo
apareció en el visor aislado; los peores fondos siguieron dominados por el
render 3D propio de Wallpaper Engine.

Para repetir esa prueba aislada:

```powershell
python -X utf8 -B bridge/verify_gpu_catalog.py --output scratch/gpu-present-copy-prueba `
  --samples 4 --warm 1.5 --present-copy --fullscreen --only we:2707903090 we:3608457742
```

Con Pillow instalado, comprueba el ciclo con clics nativos:

```powershell
python -B bridge/run.py --verify --soak-seconds 150
```

`python -B bridge/verify_startup.py` comprueba el cubo durante la carga y
la apertura de izquierda a derecha con una fuente de prueba.

`python -B bridge/verify_wallpapers.py` comprueba además la animación de
**Jugar**, la selección directa, el buscador, la rotación y las propiedades.
`python -B bridge/verify_catalog.py` prueba las bibliotecas de esta máquina,
la barra de desplazamiento, las páginas en línea, un MP4 4K y una imagen importada; también mide la RAM
privada residente del visor con el panel cerrado y abierto.

Comprueba que el visor conserva la misma ventana, que el productor anterior
termina y que el blur sigue siendo local en cada fuente. Compara el mismo
cuadro antes y después de un clic, y luego detecta cambios por animación.
Guarda PNG y `result.json` en `scratch/we-wgpu-bridge-*`. La lectura a CPU
de los PNG es exclusiva de la prueba. La comprobación omite el recorte de
RAM del arranque para que las capturas no alteren esa medición de uso normal.

Si una versión anterior del visor está abierta, puedes compilar y lanzar la
nueva sin cerrarla:

```powershell
cargo build --release --locked --target-dir target/titlebar-validation
python -B bridge/run.py --viewer target/titlebar-validation/release/ika-wgpu-wallpaper-bridge.exe
```

`node tools/titlebar-reference.cjs RUTA_LEGACY` extrae los recursos de la
rama `legacy-webview2` y genera capturas de referencia para comparar las fases.
`python -B tools/pack_titlebar.py` empaqueta los recursos. Esas herramientas
usan Playwright y Pillow y solo hacen falta al regenerar el atlas.
`node tools/crt-reference.cjs RUTA_LEGACY` captura las capas CRT en Edge y guarda
fotogramas de referencia en `scratch/crt-reference`.
`python -B tools/pack_crt.py` regenera el atlas de la luz que usa el visor.

## Reproductor MP4 aislado en Rust

El binario `mp4_player` controla la vida del proceso de vídeo desde Rust y
delega la decodificación al worker D3D11/libmpv existente. El worker usa
decodificación por hardware, una textura compartida de tamaño nativo y cero
copias por cuadro. Se eliminó el `framedrop` automático y el fundido de bucle
queda desactivado por defecto para no sacrificar FPS ni reservar otra textura
4K (33.177.600 bytes por cuadro BGRA8). La escala ocurre una sola vez en la
presentación del visor; nunca se modifica el MP4.

Pruebas aisladas, sin necesitar un archivo multimedia ni una GPU concreta:

```powershell
cargo test --lib video::tests
```

Comprueban MP4 de `1920x1080`, `2560x1440` y `3840x2160`, conservan 24/30/60
FPS, rechazan dimensiones inválidas y verifican el límite de memoria de un
cuadro. Para probar un archivo real después de compilar el backend nativo:

```powershell
cargo run --release --bin mp4_player -- .\video.mp4 .\video-mpv.exe
```

El backend recibe el PID propietario, termina junto con él y limpia cualquier
override de diagnóstico que activaría copia de frames o decodificación por CPU.

## Inicio y Minecraft

**Inicio** lee las instancias existentes de Ika, Prism, Pinecone/ElyPrism,
Freesm, Battly y las otras ubicaciones compatibles con Ika WebView2. Usa las
flechas o haz clic en una portada lateral para seleccionar una instancia.
Presiona **Jugar** o **Enter** para iniciarla.
Para ajustar la altura del carrusel, pon el ratón sobre una tarjeta y usa
**Ctrl + ↑** o **Ctrl + ↓**. Cada pulsación lo mueve un píxel lógico y la
posición se guarda al cerrar el launcher.

La primera vez, introduce el mismo nombre de Minecraft que usabas en el
lanzador anterior para conservar la identidad de jugador sin conexión.
Puedes editarlo desde **Jugador** o desde el perfil del dock. Las instancias
se abren en sus carpetas originales y reutilizan las bibliotecas, los assets
y Java que ya tenía Ika. Si Java falla, el registro queda en
`logs/ika-launcher-output.log` dentro de la carpeta de juego de la instancia.

**Crear instancia** busca e instala versiones de CurseForge y Modrinth dentro
del launcher. En las instancias **Custom**, Crear y Editar muestran los loaders
compatibles con la versión de Minecraft elegida y los runtimes de Java para
Windows. Si eliges un runtime que todavía no está instalado, Ika lo descarga
al iniciar el juego. La credencial de CurseForge se sella en el binario al
compilar: `build.rs` la lee de `IKA_SEED` o de `ika.build.local` (ignorado
por git; ver `.env.example`) y la guarda enmascarada con una clave distinta en
cada compilación. Sin ella, el lanzador funciona pero sin CurseForge.

Para comprobar la interfaz con el catálogo local sin mover el ratón, el
visor puede generar una captura antes y después de avanzar en el carrusel:

```powershell
target/release/ika-wgpu-wallpaper-bridge.exe --home-proof scratch/home-proof
```

`cargo test --release -- --include-ignored --nocapture` comprueba además el
catálogo instalado y ejecuta un JAR de prueba en una instancia temporal.
Esta prueba usa un JDK con `javac` y `jar` en PATH.
Al iniciar Minecraft, Ika desactiva las capas Vulkan implícitas del sistema
(Overwolf, OBS, Steam, ReShade...) solo para el proceso del juego. Minecraft
26.x consulta Vulkan antes de usar OpenGL; con esas capas la ventana tardaba
unos 25 s más. Añade `IkaVulkanLayers=true` a `instance.cfg` para conservarlas.

Para medir el arranque de una instancia instalada (preparación, ventana y
pantalla de título) y cerrarla después:

```powershell
$env:IKA_BENCH='nombre-de-carpeta'; cargo test --release --bin ika-wgpu-wallpaper-bridge bench_launch -- --ignored --nocapture
```

### IkaBoost (Forge 1.20.1)

En las instancias de Forge 1.20.1, Ika inyecta por defecto el agente `tools/ika-boost`
(`-javaagent`, sin tocar la carpeta `mods`). `IkaBoost=false` en `instance.cfg`
lo desactiva e `IkaBoost=true` lo activa en otras versiones de Forge. El agente:

- guarda en `%APPDATA%\IkaLauncher\cache\boost` las clases que ModLauncher y
  Mixin ya transformaron y las entrega desde disco en los siguientes arranques.
  Así no pasan por el candado global de Mixin. Una clase solo se usa después de
  dar el mismo resultado en dos arranques; se excluyen las que generan código al
  transformarse (Jasione) o dependen del estado del juego (BiggerStacks);
- fija los nombres que Mixin genera al azar en cada arranque, para que las
  clases guardadas y las nuevas encajen;
- guarda el índice de paquetes de cada jar (`packages.idx`);
- escanea las anotaciones de los mods con un hilo por núcleo en vez de uno;
- evita la comprobación de permisos de Windows al abrir cada jar.
- guarda también las clases que Mixin lee para preparar sus parches;
- en la pantalla de carga, el hilo de render procesa las tareas pendientes en
  vez de dormir hasta el siguiente fotograma (`-Dika.boost.pump=false` lo desactiva);
- memoriza los nombres de instrucciones que Mixin buscaba por reflexión en cada
  instrucción de cada mixin.
- usa Temurin 25 si está instalado y, si no, lo descarga (Temurin 21 si la
  descarga falla). Los mods compilados para Java 17 funcionan igual.
  No cambia el Java elegido a mano en Ika (`OverrideJavaLocation=true`);
  `IkaBoostJava=false` lo desactiva.

La caché se rehace si cambian los mods, la configuración, las librerías o Java.
Al cerrar Minecraft, Ika la actualiza en segundo plano: inicia el juego en un
escritorio de Windows invisible y con prioridad baja, y lo cierra cuando termina
de cargar las clases. Pulsar **Jugar** cancela esa preparación.
`-Dika.boost.verify=true` en los argumentos JVM compara cada clase de la caché
con el resultado real. Para recompilar el agente: `tools\ika-boost\build.cmd`.
`tools\ika-boost\lab.ps1` repite arranques medidos de una instancia.
