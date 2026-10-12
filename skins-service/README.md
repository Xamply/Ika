# Servicio de skins de Ika

Skins públicas por nombre de jugador para cuentas offline, **sin cuentas ni contraseñas**.

- Cada launcher sube la skin de tu perfil al darle a Jugar (solo si cambió).
- El **primer PC** que usa un nombre se lo queda. La prueba es una clave Ed25519 creada en
  ese PC y guardada cifrada con DPAPI de Windows: el archivo no sirve copiado a otro PC.
  El servicio solo guarda la clave pública.
- Los servidores con **SkinRestorer** buscan aquí la skin de cada jugador por su nombre y se
  la muestran a todos (premium, Ely.by y usuarios de Ika).

Funciona en **Cloudflare Workers + R2**, gratis para este uso.

## 1. Publicarlo (una sola vez)

Necesitas una cuenta gratuita de Cloudflare.

```powershell
cd skins-service
npm install
npx wrangler login                     # abre el navegador para iniciar sesión en Cloudflare
npx wrangler r2 bucket create ika-skins
npx wrangler deploy
```

`deploy` imprime la URL del servicio, por ejemplo `https://ika-skins.TU-USUARIO.workers.dev`.
Comprueba que responde abriéndola en el navegador: debe mostrar `{"service":"ika-skins","version":2}`.

> La primera vez que uses R2, Cloudflare puede pedirte activarlo en el panel
> (R2 → Activar). El plan gratuito incluye 10 GB, de sobra para skins (~3 KB cada una).

## 2. Ponerla en el launcher

En `src-tauri/src/device_key.rs`, cambia:

```rust
const DEFAULT_SKINS_SERVICE: &str = "";
```

por tu URL, y vuelve a compilar el launcher:

```rust
const DEFAULT_SKINS_SERVICE: &str = "https://ika-skins.TU-USUARIO.workers.dev";
```

## 3. Servidores: lo hace el launcher

No hace falta editar nada a mano. En el launcher, **Servidores → Skins compartidas**:

- **Mundo abierto desde el juego** (LAN, e4mc, Essential): automático. En las instancias
  con mods (Fabric, Quilt, Forge o NeoForge, 1.19+) el launcher añade SkinRestorer ya
  configurado. Quien no lo quiera, lo borra de su carpeta `mods` y no vuelve a aparecer.
- **Servidor en un PC**: "Elegir carpeta del servidor…". Detecta versión y loader, instala
  SkinRestorer y añade Ika a su configuración sin tocar el resto de ajustes.
- **Hosting con panel**: "Exportar para hosting" genera `mods/` y `config/` listos para
  subir desde el panel.

SkinRestorer existe para Minecraft 1.19 en adelante.

### Configuración manual (si prefieres hacerlo tú)

En `config/skinrestorer/config.json` del servidor:

1. Añade el proveedor de Ika en `providers.custom`:

```json
"custom": [
  {
    "type": "username-url",
    "name": "ika",
    "enabled": true,
    "urlTemplate": "https://ika-skins.TU-USUARIO.workers.dev/skins/{username}.png",
    "cache": { "enabled": true, "duration": 60 }
  }
]
```

2. Pon `ika` primero en la búsqueda automática al entrar (si un jugador no tiene skin en
   Ika, se sigue buscando en Mojang y Ely.by):

```json
"autoFetch": {
  "enabled": true,
  "overrideExisting": false,
  "providers": ["ika", "mojang", "ely.by"]
}
```

Reinicia el servidor. Con eso ya funciona.

### ¿Y MineSkin? No hay que hacer nada

Minecraft solo muestra las skins de otros jugadores si vienen **firmadas por Mojang**.
Nadie puede firmar con la clave de Mojang salvo Mojang, así que SkinRestorer usa
**MineSkin**, un servicio público y gratuito que devuelve la skin ya firmada por Mojang.
SkinRestorer lo hace **solo y sin configurar nada**: no es un paso de la instalación.

Opcional, solo para el dueño de un servidor grande: MineSkin limita cuántas skins *nuevas*
firma por minuto sin clave. Si en tu servidor entran muchos jugadores nuevos a la vez, una
clave gratuita (https://account.mineskin.org/keys) en `providers.mineskin.apiKey` sube ese
límite. Los usuarios del launcher nunca tienen que tocar esto.

## Costo y límites

El plan gratuito de Cloudflare es un plan publicado y permanente, no una prueba. Sus límites
(documentación de Cloudflare, Workers y R2) frente a lo que usa este servicio:

| Límite gratuito | Uso del servicio |
|---|---|
| **10 GB** de almacenamiento | Una skin pesa ~3 KB (máximo 64 KB): caben **más de 3 millones** de skins. Cientos de skins son ~1 MB. |
| **100.000 peticiones al día** | Una lectura cuando un jugador **entra** a un servidor (SkinRestorer la guarda en caché). Una escritura solo cuando alguien **cambia** su skin. |
| **1 millón** de escrituras y **10 millones** de lecturas al mes en R2 | Muy por debajo de lo anterior. |
| Tráfico de salida | Gratis. |

- **Si algún día se superara** el límite diario de peticiones, Cloudflare responde con error
  hasta el día siguiente: no cobra. Los jugadores verían la skin por defecto ese rato, y el
  launcher y el juego siguen funcionando igual.
- R2 puede pedir un método de pago al activarlo. Si se superaran sus límites mensuales, los
  precios son de centavos ($0,36 por millón de lecturas extra). Para quedarte tranquilo,
  configura una alerta de facturación en el panel de Cloudflare.
- Para pedir menos lecturas, sube la caché del proveedor `ika` en SkinRestorer
  (`"duration": 3600`, una hora).

## API

| Método | Ruta | Uso |
|---|---|---|
| `GET` | `/skins/{nombre}.png` | La skin (lo que pide SkinRestorer) |
| `PUT` | `/skins/{nombre}` | Subir/cambiar (firmado) |
| `DELETE` | `/skins/{nombre}` | Liberar el nombre (firmado) |

Las escrituras llevan `X-Ika-Pubkey`, `X-Ika-Timestamp` y `X-Ika-Signature`: una firma
Ed25519 de `ika-skin-v1\n{MÉTODO}\n{nombre}\n{timestamp}\n{sha256(cuerpo) en hex}`.
Solo se aceptan PNG de 64×64 o 64×32 de hasta 64 KB, y nombres de 2 a 16 caracteres
`[A-Za-z0-9_]` (sin distinguir mayúsculas).

### Buzón para enviar instancias de PC a PC

El launcher puede enviar una instancia a un amigo (botón Exportar del dock). La instancia
**no pasa por este servicio**: viaja por una conexión directa entre los dos launchers
(WebRTC). El servicio solo guarda unos minutos los mensajes con los que se encuentran: la
invitación y la oferta/respuesta de conexión, de pocos KB.

| Método | Ruta | Uso |
|---|---|---|
| `GET` | `/share/box/{nombre}` | Mis mensajes. Reclama el nombre si está libre (firmado) |
| `POST` | `/share/box/{destino}?from={nombre}` | Dejar un mensaje `{id, type, data}` (firmado) |
| `DELETE` | `/share/box/{nombre}?keys=a,b` | Quitar mensajes ya atendidos (firmado) |

- Mismas cabeceras de firma, sobre
  `ika-share-v1\n{MÉTODO}\n{ruta con su query}\n{timestamp}\n{sha256(cuerpo) en hex}`.
- Tipos de mensaje: `invite` (caduca a los 7 días), `offer` y `answer` (15 minutos),
  `decline` y `cancel` (1 día). Máximo 24 KB por mensaje y 40 mensajes por buzón.
- Un nombre sin skin se reclama al abrir su buzón (`names/{nombre}.json`); el dueño es el
  mismo para skins y buzón. `DELETE /skins/{nombre}` libera las tres cosas.
- Cada launcher abierto consulta su buzón una vez por minuto (más seguido mientras hay un
  envío en marcha): unas 1.440 peticiones al día por launcher, a tener en cuenta frente al
  límite gratuito de 100.000 peticiones diarias.

## Probarlo en local (sin cuenta)

```powershell
npx wrangler dev --port 8787
```

y en el launcher pon `http://127.0.0.1:8787` en `%APPDATA%\IkaLauncher\skins-service.txt`.
