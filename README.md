# ⚡ ModComparer — Comparador de Instancias (Versión Tauri + Rust)

Herramienta nativa para Windows desarrollada en **Tauri v2 + Rust** con aceleración gráfica por **GPU (WebView2)**. Diseñada para comparar instantáneamente cientos de mods de Minecraft entre diferentes versiones y mod loaders (Fabric, Forge, NeoForge, Quilt).

---

## 🚀 ¿Por qué en Tauri?

- **Rendimiento Instantáneo**: Escaneo y descompresión de metadatos de más de 200 archivos `.jar` en paralelo usando todos los núcleos del CPU con **Rayon** (en ~50 ms).
- **Desplazamiento a 60 / 120 FPS sin artefactos**: Gracias a la aceleración por hardware de WebView2, no hay franjas negras, tearing ni ghosting al deslizar con el ratón.
- **Búsquedas y filtros con latencia cero**: Filtrado en tiempo real en menos de 1 milisegundo sin bloquear el hilo de la interfaz.
- **Binario Nativo Ultra Ligero**: Un único archivo ejecutable de 10 MB (`ModComparer.exe`) que no requiere Python ni Node para funcionar.

---

## 🎯 Cómo Usarlo

### 1. Iniciar la aplicación
Simplemente haz doble clic en:
```
ModComparer.exe
```
(o en `run.bat`).

### 2. Seleccionar las dos instancias
- **Zona Izquierda (Instancia 1)**: Tu instancia base o de referencia.
- **Zona Derecha (Instancia 2)**: La nueva instancia que estás armando.
- Puedes seleccionar la carpeta principal de la instancia o la carpeta `/mods`; la herramienta detecta la subcarpeta `/mods` automáticamente.

### 3. Comparar y Revisar lo que Falta
- **Clic en cualquier mod**: Localiza automáticamente su contraparte en la otra columna, **hace scroll suave hasta él** y lo resalta con un borde luminoso azul.
- **Filtro rápido "✕ Faltan en Inst. 2"**: Te muestra exactamente los mods que tienes pendientes de descargar.
- **"📋 Copiar faltantes"**: Copia al portapapeles la lista limpia de los mods faltantes.
- **"🌐 Buscar en Modrinth"**: Abre el navegador con la búsqueda directa del mod seleccionado.
- **"🔄 Recargar"**: Vuelve a escanear ambas carpetas instantáneamente cuando descargues nuevos mods.
