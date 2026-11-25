# Por qué Ika 1.6 es tan rápido y ligero

Medido el 25/11/2026 en el commit `v1.6.0`, abriendo el launcher con
`python -B bridge/run.py` y sin tocarlo durante 2 minutos.

**Equipo:** RTX 4070 SUPER, Ryzen 9 5900XT y 32 GB de RAM.
**Fondo activo:** vídeo MP4 *Chainsaw Man Girls* (115 MB, del Workshop).

## Los números

| Qué | Resultado |
|---|---|
| Ventana visible desde que se lanza | ~1 s |
| GPU 3D del launcher, en reposo | 5,4–8 % |
| GPU del vídeo (`video-mpv.exe`) | ~11 % en el motor **Video Decode**; 0,1 % en 3D |
| CPU del launcher | ~4,6 % de **un** núcleo; ~0,2 % del procesador entero |
| RAM privada del launcher, recién abierto | 11,9 MB |
| RAM privada, usándolo con foco | ~14–16 MB |
| RAM privada, tras minimizar y volver | **9,7 MB**, y se mantiene ahí |

La "Memoria" del Administrador de tareas de Windows 11 es la **RAM privada en
uso** (*private working set*). Es la cifra de esta tabla.

## 1. No hay navegador dentro

La 1.5.x era Tauri + WebView2: un Edge completo con varios procesos (GPU,
renderizador, red) y un motor de HTML, CSS y JavaScript, diseños, recolector
de basura… Solo para *existir* ya cuesta cientos de MB.

La 1.6 no carga HTML, CSS, JavaScript ni WebView. La interfaz es código Rust
que le dice directamente a la GPU qué dibujar con `wgpu` sobre DirectX 12.
Iconos, textos y degradados van empaquetados en atlas de texturas, así que
dibujar la barra o un panel son unas pocas instrucciones a la GPU, no un
motor de diseño recalculando estilos.

## 2. Solo dibuja cuando algo cambia

Un juego, o un navegador animando, dibuja la pantalla entera 60–144 veces
por segundo aunque nada cambie. Ika no.

El bucle de eventos (`winit`) usa `ControlFlow::Wait` y `WaitUntil`
(`src/main.rs`). El launcher **duerme** hasta que pasa una de estas cosas:
- llega un fotograma nuevo del fondo,
- toca el siguiente paso de una animación (reloj, barra de título, dock),
- el usuario mueve el ratón o pulsa una tecla.

Si no pasa nada, no dibuja nada. Cada "despertar" se calcula como la
siguiente fecha necesaria, no con un temporizador fijo.

## 3. El vídeo nunca pasa por la CPU (la gran razón del ~15 % de GPU)

Este es el truco principal:

1. `video-mpv.exe` decodifica el MP4 con el **chip de vídeo dedicado** de la
   tarjeta (NVDEC), no con los núcleos 3D. Por eso aparece en el motor
   *Video Decode* (~11 %) y casi nada en *3D* (0,1 %).
2. Escribe cada fotograma directamente en una **textura compartida** de la
   GPU (`d3d11va-zero-copy`).
3. El launcher abre esa misma textura (`OpenSharedHandle`, `src/shared.rs`) y
   la usa tal cual. Dos *fences* de GPU coordinan quién escribe y quién lee.

El fotograma nunca baja a la RAM ni vuelve a subir: no hay copias. La GPU
hace el trabajo para el que está diseñada y cada parte usa su propio motor.

Además:
- **Solo presenta a la cadencia del vídeo.** Si el MP4 es de 60 FPS, no
  redibuja 120 veces; si no hay fotograma nuevo, no compone.
- **DirectComposition** (`src/video_planes.rs`): Windows puede mostrar el
  vídeo y la interfaz como **dos capas** de la misma ventana y combinarlas él
  mismo. La interfaz se reutiliza entre fotogramas en vez de redibujarse.
- **Compilador FXC** para los shaders: en esta RTX 4070 SUPER redujo el
  tiempo de composición frente a DXC (ver `docs/ARQUITECTURA.md`).

Con escenas de Wallpaper Engine pasa lo mismo: el motor dibuja directamente
en una textura compartida (`direct_alias`) y el launcher la lee sin copiarla.
En la medición, eso bajó la GPU 3D del productor del 19,5 % al 0,2 %.

Resultado: el launcher (~6 % de 3D) más el vídeo (~11 % de decodificación,
en otro motor) dan el ~10–15 % que ves en el historial de GPU de System
Informer.

## 4. Después de un rato baja a ~10 MB de RAM

Windows mide la "Memoria" como las páginas que el proceso tiene **ahora** en
la RAM física. Al arrancar se tocan muchas páginas que ya no se volverán a
usar: carga de shaders, inicialización de DirectX, lectura del catálogo,
decodificación de iconos…

`src/resident_memory.rs` hace dos cosas:
- **1 s después de terminar el arranque**, llama a `EmptyWorkingSet` y le dice
  a Windows: "saca de la RAM las páginas que no estoy usando".
- **Cuando vuelve a estar quieto**, 2 s sin animaciones ni paneles abiertos,
  tras haber perdido el foco o haber estado ocupado, repite la limpieza solo
  si su RAM privada pasa de 12 MB.

Medido:

```
12 s tras abrir                  11,9 MB
con foco, usándolo               16,0 MB
minimizado                       16,1 MB  ← sin redibujar, espera
restaurado, 3 s después           9,7 MB  ← limpieza al volver a reposo
restaurado, 23 s después          9,8 MB  ← se mantiene
```

Se mantiene porque, en reposo, el launcher solo toca un conjunto pequeño de
memoria: el estado de la interfaz y el bucle de eventos. Los píxeles del fondo
y de la interfaz viven en la **memoria de la GPU** (VRAM), no en la RAM.

Las reglas del código evitan el efecto contrario: **nunca** limpia durante
una animación o con un panel abierto, y un fotograma o un tic del reloj no
vuelven a activarla. Si lo hiciera, Windows tendría que volver a cargar
páginas en plena reproducción y se notarían tirones.

### Lo que esa cifra no cuenta (para ser justos)

- La **memoria comprometida** del launcher es de ~306 MB: memoria reservada,
  sobre todo por el controlador de la GPU, que no ocupa RAM física mientras no
  se usa. No es lo que muestra el Administrador de tareas.
- `video-mpv.exe` es un proceso aparte, con ~19 MB.
- Wallpaper Engine, si el fondo es una escena, también va aparte.
- La limpieza no hace que el programa necesite menos memoria. Devuelve a
  Windows lo que no está usando, y Windows puede dárselo a otro programa.

## 5. Por qué responde tan rápido

- **Arranque en ~1 s:** DirectX se prepara con la ventana oculta, los shaders
  ya vienen compilados dentro del `.exe` (`build.rs`) y el fondo inicial se
  publica sin esperar al resto del catálogo. Mientras tanto se ve el cubo de
  carga.
- **Clics al instante:** un clic es un evento nativo de Windows que llega
  directo a Rust. No pasa por la cadena del navegador (evento del DOM,
  JavaScript, IPC de Tauri, Rust y vuelta).
- **Nada bloquea la interfaz:** descargas, catálogo y cierre de procesos van
  en hilos o procesos aparte.
- **Miniaturas solo de lo visible:** el selector de fondos carga solo las que
  se ven y libera sus texturas al cerrarse.

## En una frase

Ika 1.6 hace que cada pieza haga solo su trabajo: la GPU dibuja y decodifica
sin copias, el procesador duerme hasta que hay algo que hacer y la RAM
devuelve a Windows lo que no usa. Sin un navegador en medio que lo pague
todo.