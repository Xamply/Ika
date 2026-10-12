# Rendimiento y escalado: estado del trabajo

Guardado el 7 de octubre de 2026, al interrumpir la sesión para apagar el PC.
PC medido: Ryzen 9 5900XT, RTX 4070 SUPER, 32 GB, monitor 2560×1440 a 240 Hz + monitor 1920×1080.

## Qué causaba los tirones (medido, no supuesto)

1. **Reglas CSS con `:has(...)`** (la causa principal). Once reglas preguntaban "¿qué sección está
   abierta?" con `.app-root:has(#view-x.active)` / `:has(#view-x:not(.hidden))`. Con eso, cada vez
   que *cualquier* elemento del launcher ganaba o perdía la clase `hidden` o `active` (un icono de
   mod, una pestaña, una tarjeta), el navegador recalculaba el estilo de toda la interfaz: unos
   2.000 elementos, 7–17 ms cada vez, varias veces por cada cambio de instancia.
2. **Escaneo de mods sin caché.** `scan_directory` leía y calculaba el hash de todos los .jar en
   cada visita: entre 1,3 y 5 s por instancia, con los 32 hilos ocupados (por eso se resentía
   hasta el fondo). Además la compilación de desarrollo de Rust no estaba optimizada.
3. **Packs**: cada listado devolvía 500–600 KB de texto (los iconos de los resource packs iban
   incrustados) y volvía a calcular el hash de cada shader al reabrir el launcher.
4. **Placas del Gestor**: su luz animada obliga a redibujar capas SVG con desenfoque 30 veces por
   segundo. Es el mayor coste fijo de esa sección (214 fps con placas, 235 sin ellas).

## Hecho y comprobado

- **`:has()` sustituido por un atributo** `data-view` en `#app`, que pone `switchView`
  (`src/style.css`, `src/main.js`, `index.html`). Medido antes y después con el launcher real:

  | | Antes | Después |
  |---|---|---|
  | Tarea más larga al cambiar de instancia | 17–27 ms | 5–8 ms |
  | Peor salto de fotograma al cambiar de instancia | 25–38 ms | 13 ms |
  | Fotogramas lentos por cambio de instancia | 20–44 | 4–7 (igual que en reposo) |

- **Compilación de desarrollo optimizada** (`src-tauri/Cargo.toml`, perfil `dev`). Compila y el
  launcher arrancó con ella.

## Comprobado el 7 de octubre por la tarde (segunda sesión)

- **Caché persistente de escaneo de mods** (`src-tauri/src/disk_cache.rs` nuevo, `scanner.rs`).
  Recuerda cada .jar mientras no cambien su tamaño y fecha. Se guarda en
  `%APPDATA%\ModComparer\mod_scan_cache.json`. Medido llamando a `scan_directory` para las 55
  instancias (2.616 mods): **443 ms en total**, la peor 28 ms (301 mods). Antes eran 1,3–5 s por
  instancia. En la primera pasada tras arrancar, la peor fue 428 ms.
- **Caché de packs e iconos como archivo** (`manager.rs`, `main.js`). Los iconos se ven en la
  pestaña Resource Packs (6 de 6 cargados desde `pack_icons\`). El listado más grande pesa 8 KB
  (antes 500–600 KB) y tarda como mucho 47 ms.
- **Expresiones regulares compiladas una sola vez** (`scanner.rs`, `knowledge.rs`).
- **Escalado por resolución** (`src-tauri/src/lib.rs`, función `apply_ui_zoom`). Probado moviendo
  la ventana al monitor 1080p y de vuelta, maximizada y en ventana: el zoom pasa a 0,75 y vuelve
  a 1, la página mantiene 2560 de ancho lógico y la composición es la misma. El fondo de
  Wallpaper Engine sigue alineado, también con desplazamiento y zoom de fondo aplicados.
  Única diferencia: la barra de tareas mide lo mismo en ambos monitores, así que en 1080p la
  página queda 20 px lógicos más baja (1362 frente a 1382).

## Gestor en reposo: hecho el 7 de octubre por la tarde (`src/manager-plates.js`)

Con el Gestor abierto y sin tocar nada, el proceso gráfico del navegador gastaba el 73–75 % de un
núcleo y su hilo principal estaba ocupado el 62 % del tiempo (en Inicio, 20 %). Una traza dice en
qué: las ondas de energía 21 puntos, los brillos SVG de los bordes 14, el resto es común a todas
las secciones. La isla 3D casi no cuesta.

- **Ondas de energía dibujadas con WebGL.** Como trazos de un lienzo 2D, las 20 líneas finas (300
  segmentos cada una, nuevas en cada fotograma) eran lo más caro de la sección. Ahora son
  triángulos, con el mismo desenfoque. Comparadas con la versión anterior a tiempo congelado, la
  imagen es la misma (diferencia media 0,1 sobre 255). Si no hay WebGL 2, se usa el dibujo de
  antes (probado forzándolo).
- **La escena ya no hace trabajar a la página 240 veces por segundo para dibujar 30.** Pedía un
  fotograma en cada refresco de pantalla solo para mirar si tocaba dibujar. Ahora lo pide cuando
  toca. Sigue a 30 fps (medido: 29,8–30 en los dos monitores).

  | Gestor en reposo | Antes | Después |
  |---|---|---|
  | Hilo principal del proceso gráfico | 61,8 % | 41,0 % |
  | CPU del proceso gráfico | 73–75 % | 52–57 % |
  | Hilo principal de la página | 17,6 % | 14,0–14,3 % |
  | Fotogramas que procesa la página por segundo | 222 | 57 |

- **Descartado, con medida: mover la luz de los bordes con el compositor** (el punto 1 de la
  lista anterior). Lo construí entero (cada brillo como máscara fija sobre una tira de luz que
  solo se desplaza) y se veía igual, pero era más caro: el hilo gráfico bajaba 5 puntos y el
  compositor, la página y su compositor subían 10 entre los tres. Además la primera versión
  agotó la memoria de capas y dejaba placas sin pintar. Está deshecho; los bordes se dibujan
  como antes. No merece otro intento por ese camino.

## Pendiente (por orden de impacto)

1. **Brillos SVG de los bordes de las placas**: 14 puntos del hilo gráfico. Lo único que queda por
   probar es actualizar las líneas finas menos veces por segundo (los halos ya van a 7,5); cambia
   la cadencia de la luz, así que hay que mirarlo en pantalla antes de dejarlo.
2. **Líneas animadas de la barra de título y el dock**: redibujan degradados SVG grandes 24 veces
   por segundo en todas las secciones (parte de ese 20 % que el proceso gráfico gasta en Inicio).
   El arreglo de máscaras no sirve (ver arriba); habría que dibujarlas en un lienzo WebGL como
   las ondas, o bajarles la frecuencia.
3. **Primer fotograma al cambiar de sección**: aún hay saltos de 50–55 ms al entrar en Gestor y
   Comparador y de 88 ms al entrar en Consola (ahí son ~2.500 líneas maquetadas de golpe).
4. **Comparador**: abrir el selector de instancia bloquea 27 ms y elegir una, 19 ms; al elegir
   quedan saltos de 25–58 ms.
5. **Arranque**: `instance_meta` se pide 50 veces (80 ms cada una) y
   `get_all_downloaded_wallpapers` tarda 630 ms y devuelve 319 KB.
6. Dar prioridad baja a los hilos de escaneo, para que un primer escaneo nunca compita con el fondo.
7. Comprobar todo en la compilación final (`release`), no solo en desarrollo. **Nada de lo de hoy
   está probado en `release`.**
8. En ventana (sin maximizar) a unos 1.640 px de ancho lógico, la fila de datos de abajo queda
   pegada a las tarjetas de cuenta y de Fondos Animados. Visto de pasada al probar el escalado;
   no sé si ya pasaba antes.

## Notas

- El primer `npm run tauri dev` tras el cambio de `Cargo.toml` ya recompiló todas las dependencias;
  los siguientes tardan lo normal.
- Usé un puerto de depuración temporal en `tauri.conf.json` para sacar las trazas. **Ya está
  quitado**, y no queda código de medición en el proyecto.
- Para saber qué cuesta de verdad: una traza del navegador (`Tracing.start` por ese puerto, contra
  el destino del navegador) sumando el tiempo propio por hilo. Los fotogramas largos medidos desde
  la página varían demasiado de una pasada a otra para comparar dos versiones.
- Si algo de la caché diera problemas, basta con borrar `mod_scan_cache.json` y `pack_cache.json`
  de `%APPDATA%\ModComparer\`: se regeneran solos.
- De esta misma sesión, ya entregado antes: zona atmosférica en lugar del círculo, entrada del
  Gestor sin salto de ventanas, disposición de ventanas guardada, paginación en MotionBGs.
