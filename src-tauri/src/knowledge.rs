use serde::{Deserialize, Serialize};

pub struct KnownMod {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub tags: &'static [&'static str],
    pub description: &'static str,
    pub is_api: bool,
    pub equivalents: &'static [&'static str],
}

pub static KNOWN_MODS: &[KnownMod] = &[
    // RENDIMIENTO - RENDERIZADO (SODIUM / EMBEDDIUM / IRIS / OCULUS)
    KnownMod {
        id: "sodium",
        name: "Sodium",
        category: "Rendimiento",
        tags: &["Renderizado", "FPS", "Chunks"],
        description: "Reescribe el motor de renderizado de bloques y chunks aumentando drasticamente los FPS.",
        is_api: false,
        equivalents: &["embeddium", "rubidium", "magnesium", "xenon"],
    },
    KnownMod {
        id: "embeddium",
        name: "Embeddium",
        category: "Rendimiento",
        tags: &["Renderizado", "FPS", "Chunks"],
        description: "Port de Sodium para Forge y NeoForge con mejoras de compatibilidad.",
        is_api: false,
        equivalents: &["sodium", "rubidium", "magnesium", "xenon"],
    },
    KnownMod {
        id: "rubidium",
        name: "Rubidium",
        category: "Rendimiento",
        tags: &["Renderizado", "FPS", "Chunks"],
        description: "Port legacy de Sodium para Forge.",
        is_api: false,
        equivalents: &["sodium", "embeddium", "magnesium"],
    },
    KnownMod {
        id: "iris",
        name: "Iris Shaders",
        category: "Gráficos, Cámara y Animaciones",
        tags: &["Shaders", "Iluminacion", "Visuales"],
        description: "Permite usar paquetes de shaders modernos con compatibilidad total con Sodium.",
        is_api: false,
        equivalents: &["oculus"],
    },
    KnownMod {
        id: "oculus",
        name: "Oculus",
        category: "Gráficos, Cámara y Animaciones",
        tags: &["Shaders", "Iluminacion", "Visuales"],
        description: "Port de Iris Shaders para Forge y NeoForge.",
        is_api: false,
        equivalents: &["iris"],
    },

    // RENDIMIENTO - CHUNKS / MEMORIA / OPTIMIZACION
    KnownMod {
        id: "c2me",
        name: "C2ME",
        category: "Rendimiento",
        tags: &["Chunks", "Multithreading", "Mundo"],
        description: "Optimiza la carga y generacion de chunks mediante procesamiento concurrente multihilo.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "ferritecore",
        name: "FerriteCore",
        category: "Rendimiento",
        tags: &["Memoria", "RAM", "Modelos"],
        description: "Reduce de forma masiva el consumo de RAM de los modelos de bloques y texturas.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "modernfix",
        name: "ModernFix",
        category: "Rendimiento",
        tags: &["Memoria", "Fixes", "Carga"],
        description: "Acelera el tiempo de arranque del juego y arregla fugas de memoria en Forge y NeoForge.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "entityculling",
        name: "Entity Culling",
        category: "Rendimiento",
        tags: &["Entidades", "FPS", "Renderizado"],
        description: "Evita renderizar entidades y bloques de mosaico ocultos detras de paredes.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "immediatelyfast",
        name: "ImmediatelyFast",
        category: "Rendimiento",
        tags: &["HUD", "Texto", "Renderizado"],
        description: "Optimiza el renderizado de interfaces, texto, mapas y entidades del HUD.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "badoptimizations",
        name: "BadOptimizations",
        category: "Rendimiento",
        tags: &["Luces", "Ticks", "Optimizacion"],
        description: "Elimina calculos redundantes en la actualizacion de luces y renderizado interno.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "lithium",
        name: "Lithium",
        category: "Rendimiento",
        tags: &["Fisicas", "IA Mobs", "Chunks"],
        description: "Optimiza las fisicas del servidor, IA de criaturas y deteccion de colisiones.",
        is_api: false,
        equivalents: &["canary", "radium"],
    },
    KnownMod {
        id: "canary",
        name: "Canary",
        category: "Rendimiento",
        tags: &["Fisicas", "IA Mobs", "Chunks"],
        description: "Port de Lithium para Forge.",
        is_api: false,
        equivalents: &["lithium", "radium"],
    },
    KnownMod {
        id: "radium",
        name: "Radium",
        category: "Rendimiento",
        tags: &["Fisicas", "IA Mobs", "Chunks"],
        description: "Port de Lithium para Forge.",
        is_api: false,
        equivalents: &["lithium", "canary"],
    },
    KnownMod {
        id: "noisium",
        name: "Noisium",
        category: "Rendimiento",
        tags: &["Mundo", "Chunks", "Generacion"],
        description: "Optimiza la funcion de ruido matematico para acelerar la generacion de biomas y cuevas.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "krypton",
        name: "Krypton",
        category: "Rendimiento",
        tags: &["Red", "Paquetes", "Multiplayer"],
        description: "Optimiza la capa de red y envio de paquetes de datos entre cliente y servidor.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "dynamicfps",
        name: "Dynamic FPS",
        category: "Rendimiento",
        tags: &["Bateria", "Segundo Plano", "FPS"],
        description: "Reduce los FPS automaticamente cuando Minecraft esta en segundo plano para ahorrar energia.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "memoryleakfix",
        name: "Memory Leak Fix",
        category: "Rendimiento",
        tags: &["Memoria", "RAM", "Fixes"],
        description: "Soluciona varias fugas de memoria nativas de Minecraft.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "chunky",
        name: "Chunky",
        category: "Rendimiento",
        tags: &["Chunks", "Precarga", "Rendimiento", "Optimizacion"],
        description: "Genera y precarga chunks del mundo de forma anticipada para erradicar el lag al explorar.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "modfixer",
        name: "ModFixer",
        category: "Rendimiento",
        tags: &["Mixins", "Compatibilidad", "Optimizacion", "Estabilidad"],
        description: "Motor autónomo de compatibilidad que mitiga conflictos de Mixins y optimiza la estabilidad y rendimiento.",
        is_api: false,
        equivalents: &["mod-fixer", "mod_fixer"],
    },
    KnownMod {
        id: "spark",
        name: "Spark",
        category: "Calidad de Vida",
        tags: &["Diagnostico", "Lag", "Perfilador"],
        description: "Perfilador de rendimiento para detectar que mods o entidades causan lag o tirones.",
        is_api: false,
        equivalents: &[],
    },

    // VISORES DE RECETAS (JEI / REI / EMI)
    KnownMod {
        id: "jei",
        name: "Just Enough Items (JEI)",
        category: "Calidad de Vida",
        tags: &["Recetas", "Items", "Inventario"],
        description: "Visor clasico de recetas de crafteo, usos e informacion de objetos.",
        is_api: false,
        equivalents: &["roughlyenoughitems", "rei", "emi"],
    },
    KnownMod {
        id: "roughlyenoughitems",
        name: "Roughly Enough Items (REI)",
        category: "Calidad de Vida",
        tags: &["Recetas", "Items", "Inventario"],
        description: "Visor de recetas e inventario altamente configurable.",
        is_api: false,
        equivalents: &["jei", "emi"],
    },
    KnownMod {
        id: "rei",
        name: "Roughly Enough Items (REI)",
        category: "Calidad de Vida",
        tags: &["Recetas", "Items", "Inventario"],
        description: "Visor de recetas e inventario altamente configurable.",
        is_api: false,
        equivalents: &["jei", "emi"],
    },
    KnownMod {
        id: "emi",
        name: "EMI",
        category: "Calidad de Vida",
        tags: &["Recetas", "Crafteo", "Arbol"],
        description: "Visor de recetas moderno con arboles de produccion integrados y calculo de costes.",
        is_api: false,
        equivalents: &["jei", "roughlyenoughitems", "rei"],
    },

    // MAPAS Y MINIMAPAS
    KnownMod {
        id: "xaerominimap",
        name: "Xaero's Minimap",
        category: "Interfaz",
        tags: &["Minimapa", "Waypoints", "Navegacion"],
        description: "Minimapa circular o cuadrado fluido con waypoints y radar de criaturas.",
        is_api: false,
        equivalents: &["journeymap", "map_atlases"],
    },
    KnownMod {
        id: "xaerosworldmap",
        name: "Xaero's World Map",
        category: "Interfaz",
        tags: &["Mapa", "Exploracion", "Waypoints"],
        description: "Mapa del mundo a pantalla completa que se integra con Xaero's Minimap.",
        is_api: false,
        equivalents: &["journeymap"],
    },
    KnownMod {
        id: "journeymap",
        name: "JourneyMap",
        category: "Interfaz",
        tags: &["Mapa", "Minimapa", "Waypoints"],
        description: "Mapeador completo en tiempo real con visor web y minimapa.",
        is_api: false,
        equivalents: &["xaerominimap", "xaerosworldmap"],
    },

    // ACCESORIOS / BAUBLES
    KnownMod {
        id: "curios",
        name: "Curios API",
        category: "APIs y Dependencias",
        tags: &["Accesorios", "Slots", "Libreria"],
        description: "Gestion de slots de accesorios (anillos, collares, capas) para Forge y NeoForge.",
        is_api: true,
        equivalents: &["trinkets", "accessories"],
    },
    KnownMod {
        id: "trinkets",
        name: "Trinkets",
        category: "APIs y Dependencias",
        tags: &["Accesorios", "Slots", "Libreria"],
        description: "Gestion de slots de accesorios para Fabric y Quilt.",
        is_api: true,
        equivalents: &["curios", "accessories"],
    },
    KnownMod {
        id: "accessories",
        name: "Accessories",
        category: "APIs y Dependencias",
        tags: &["Accesorios", "Slots", "Multiplatform"],
        description: "Sistema universal unificado de accesorios para Fabric y NeoForge.",
        is_api: true,
        equivalents: &["curios", "trinkets"],
    },

    // HUD / INFORMACION DE BLOQUES (JADE / WTHIT / TOP)
    KnownMod {
        id: "jade",
        name: "Jade",
        category: "Calidad de Vida",
        tags: &["HUD", "Bloques", "Tooltips"],
        description: "Muestra informacion detallada sobre el bloque o entidad al que estas mirando.",
        is_api: false,
        equivalents: &["wthit", "theoneprobe"],
    },
    KnownMod {
        id: "wthit",
        name: "WTHIT",
        category: "Calidad de Vida",
        tags: &["HUD", "Bloques", "Tooltips"],
        description: "Visor de informacion de bloques y entidades (What The Hell Is That).",
        is_api: false,
        equivalents: &["jade", "theoneprobe"],
    },
    KnownMod {
        id: "theoneprobe",
        name: "The One Probe",
        category: "Calidad de Vida",
        tags: &["HUD", "Bloques", "Tooltips"],
        description: "Sonda de informacion visual in-game sobre bloques.",
        is_api: false,
        equivalents: &["jade", "wthit"],
    },

    // TEXTURAS CIT (OPTIFINE REPLACEMENTS)
    KnownMod {
        id: "citresewn",
        name: "CIT Resewn",
        category: "Gráficos, Cámara y Animaciones",
        tags: &["Texturas", "Items", "Optifine"],
        description: "Permite usar paquetes de texturas con Custom Item Textures (CIT) sin OptiFine.",
        is_api: false,
        equivalents: &["citresewnforge"],
    },
    KnownMod {
        id: "citresewnforge",
        name: "CIT Resewn Forge",
        category: "Gráficos, Cámara y Animaciones",
        tags: &["Texturas", "Items", "Optifine"],
        description: "Port de CIT Resewn para Forge y NeoForge.",
        is_api: false,
        equivalents: &["citresewn"],
    },

    // QoL Y CONTROL
    KnownMod {
        id: "appleskin",
        name: "AppleSkin",
        category: "Calidad de Vida",
        tags: &["Comida", "Saturacion", "HUD"],
        description: "Visualiza la saturacion y curacion que aporta cada alimento en la barra de hambre.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "mousetweaks",
        name: "Mouse Tweaks",
        category: "Calidad de Vida",
        tags: &["Inventario", "Raton", "Arrastre"],
        description: "Mejora el arrastre de items con el boton derecho e izquierdo en inventarios.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "inventoryprofilesnext",
        name: "Inventory Profiles Next",
        category: "Calidad de Vida",
        tags: &["Inventario", "Ordenacion", "Cofres"],
        description: "Ordenacion inteligente de inventarios y cofres con un solo clic o atajo de teclado.",
        is_api: false,
        equivalents: &["inventorysorter", "itemscroller"],
    },
    KnownMod {
        id: "controlling",
        name: "Controlling",
        category: "Calidad de Vida",
        tags: &["Teclas", "Controles", "Menu"],
        description: "Buscador de atajos de teclado y detector de conflictos en el menu de controles.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "shulkerboxtooltip",
        name: "Shulker Box Tooltip",
        category: "Calidad de Vida",
        tags: &["Cajas Shulker", "Previsualizacion", "Inventario"],
        description: "Muestra una vista previa del contenido de cajas de shulker al pasar el raton por encima.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "legendarytooltips",
        name: "Legendary Tooltips",
        category: "Interfaz",
        tags: &["Tooltips", "Rareza", "Estilo"],
        description: "Bordes decorativos y fondos personalizados para tooltips de items segun su rareza.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "zoomify",
        name: "Zoomify",
        category: "Calidad de Vida",
        tags: &["Zoom", "Camara", "Optifine"],
        description: "Zoom suave y configurable con la rueda del raton similar o superior a OptiFine.",
        is_api: false,
        equivalents: &["okzoomer", "logicalzoom", "wzoom"],
    },

    // GENERACION DE MUNDO
    KnownMod {
        id: "terralith",
        name: "Terralith",
        category: "Mundo y Generación",
        tags: &["Biomas", "Terreno", "Overworld"],
        description: "Revoluciona el Overworld con casi 100 nuevos biomas realistas y cañones espectaculares.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "tectonic",
        name: "Tectonic",
        category: "Mundo y Generación",
        tags: &["Terreno", "Cordilleras", "Acantilados"],
        description: "Genera cordilleras gigantescas, rios subterraneos y terreno suave estilo meseta.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "biomesoplenty",
        name: "Biomes O' Plenty",
        category: "Mundo y Generación",
        tags: &["Biomas", "Plantas", "Arboles"],
        description: "Añade decenas de biomas unicos tanto en el Overworld como en el Nether.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "nullscape",
        name: "Nullscape",
        category: "Mundo y Generación",
        tags: &["End", "Dimensiones", "Biomas"],
        description: "Reimagina la dimension del End con paisajes fractales y cadenas montañosas cristalinas.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "amplifiednether",
        name: "Amplified Nether",
        category: "Mundo y Generación",
        tags: &["Nether", "Montañas", "Altura"],
        description: "Aumenta la altura del Nether a 256 bloques con biomas verticales asombrosos.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "whendungeonsarise",
        name: "When Dungeons Arise",
        category: "Mundo y Generación",
        tags: &["Estructuras", "Mazmorras", "Aventura"],
        description: "Añade gigantescas fortalezas, palacios y mazmorras generadas por el mundo.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "townsandtowers",
        name: "Towns and Towers",
        category: "Mundo y Generación",
        tags: &["Aldeas", "Pillagers", "Estructuras"],
        description: "Mejora el diseño de aldeas y puestos de saqueadores adaptados a cada bioma.",
        is_api: false,
        equivalents: &[],
    },

    // DEPENDENCIAS Y LIBRERIAS
    KnownMod {
        id: "clothconfig",
        name: "Cloth Config v13/v15",
        category: "APIs y Dependencias",
        tags: &["Configuracion", "Libreria", "Pantallas"],
        description: "Libreria de pantallas de configuracion para mods cliente y servidor.",
        is_api: true,
        equivalents: &["clothconfig2", "cloth_config"],
    },
    KnownMod {
        id: "clothconfig2",
        name: "Cloth Config",
        category: "APIs y Dependencias",
        tags: &["Configuracion", "Libreria"],
        description: "Libreria de pantallas de configuracion para mods.",
        is_api: true,
        equivalents: &["clothconfig", "cloth_config"],
    },
    KnownMod {
        id: "cloth_config",
        name: "Cloth Config",
        category: "APIs y Dependencias",
        tags: &["Configuracion", "Libreria"],
        description: "Libreria de pantallas de configuracion para mods.",
        is_api: true,
        equivalents: &["clothconfig", "clothconfig2"],
    },
    KnownMod {
        id: "architectury",
        name: "Architectury API",
        category: "APIs y Dependencias",
        tags: &["Multiplataforma", "Libreria"],
        description: "Libreria puente para mods multi-loader (Fabric, Forge, NeoForge).",
        is_api: true,
        equivalents: &[],
    },
    KnownMod {
        id: "fabricapi",
        name: "Fabric API",
        category: "APIs y Dependencias",
        tags: &["Hooks", "Eventos", "Base"],
        description: "Capa de ganchos (hooks) esenciales para mods de Fabric.",
        is_api: true,
        equivalents: &[],
    },
    KnownMod {
        id: "balm",
        name: "Balm",
        category: "APIs y Dependencias",
        tags: &["Waystones", "Libreria", "Blay"],
        description: "Capa de abstraccion para mods de Blay09 (Waystones, Better Village, etc.).",
        is_api: true,
        equivalents: &["balmforge", "balmfabric"],
    },
    KnownMod {
        id: "geckolib",
        name: "GeckoLib",
        category: "APIs y Dependencias",
        tags: &["Animaciones", "Modelos", "3D"],
        description: "Motor de animaciones 3D fluidas para entidades, armaduras y bloques.",
        is_api: true,
        equivalents: &[],
    },
    KnownMod {
        id: "searchables",
        name: "Searchables",
        category: "APIs y Dependencias",
        tags: &["Buscador", "Libreria", "UI"],
        description: "Libreria para añadir barras de busqueda complejas con filtros a cualquier interfaz.",
        is_api: true,
        equivalents: &[],
    },
    KnownMod {
        id: "playeranimationlib",
        name: "Player Animation Lib",
        category: "APIs y Dependencias",
        tags: &["Animaciones", "Jugador", "Libreria"],
        description: "Libreria de animaciones avanzadas en tercera persona para el jugador.",
        is_api: true,
        equivalents: &[],
    },
    KnownMod {
        id: "citadel",
        name: "Citadel",
        category: "APIs y Dependencias",
        tags: &["AlexMobs", "Libreria", "Animacion"],
        description: "Libreria de entidades y animaciones base para mods como Alex's Mobs.",
        is_api: true,
        equivalents: &[],
    },
    KnownMod {
        id: "better-looting",
        name: "Better Looting",
        category: "Interfaz",
        tags: &["UI", "Loot", "Interfaz", "Inventario"],
        description: "Mejora visual de la interfaz de apertura de cofres y contenedores con animaciones y previsualización de botín.",
        is_api: false,
        equivalents: &["betterlooting", "better_looting"],
    },
    KnownMod {
        id: "cataclysm",
        name: "L_Enders_Cataclysm",
        category: "Contenido",
        tags: &["Contenido", "Jefes", "Aventura", "Mazmorras"],
        description: "Añade mazmorras colosales, jefes desafiantes y armas míticas a la aventura.",
        is_api: false,
        equivalents: &["l_enders_cataclysm", "cataclysmic", "cataclysmmod"],
    },
    KnownMod {
        id: "azurelib",
        name: "AzureLib",
        category: "APIs y Dependencias",
        tags: &["Animacion", "Libreria", "Modelos", "GeckoLib"],
        description: "Libreria de animación y renderizado de entidades y armaduras en formato GeckoLib/Azure.",
        is_api: true,
        equivalents: &["azurelibarmor", "azurelib-neo", "azurelibneo", "azurelibforge"],
    },
    KnownMod {
        id: "easy-mob-spawn-control",
        name: "Easy Mob Spawn Control",
        category: "Rendimiento",
        tags: &["Mobs", "Spawns", "Optimizacion", "Lag"],
        description: "Controla y limita la generacion de criaturas para reducir el lag y optimizar el rendimiento del servidor y cliente.",
        is_api: false,
        equivalents: &["easymobspawncontrol", "easy_mob_spawn_control", "easymobspawn"],
    },
    KnownMod {
        id: "tctcore",
        name: "TCTCore",
        category: "APIs y Dependencias",
        tags: &["Libreria", "Core", "Dependencias"],
        description: "Libreria y nucleo tecnico requerido por los mods y complementos del equipo TCT.",
        is_api: true,
        equivalents: &["tct_core", "tct-core"],
    },
    KnownMod {
        id: "better-third-person",
        name: "Better Third Person",
        category: "Calidad de Vida",
        tags: &["Camara", "Vista", "QoL"],
        description: "Permite rotar libremente la camara en tercera persona en 360 grados sin cambiar la direccion del jugador.",
        is_api: false,
        equivalents: &["betterthirdperson", "better_third_person"],
    },
    KnownMod {
        id: "afk-camera",
        name: "AFK Camera",
        category: "Calidad de Vida",
        tags: &["Camara", "AFK", "Cinematica"],
        description: "Activa una camara cinematica que orbita al jugador suavemente cuando esta ausente (AFK).",
        is_api: false,
        equivalents: &["afkcam", "afkcamera", "afkstation"],
    },
    KnownMod {
        id: "cinematic-cataclysm",
        name: "Cinematic Cataclysm",
        category: "Calidad de Vida",
        tags: &["Camara", "Jefes", "Cinematica"],
        description: "Añade secuencias y efectos de camara cinematica durante los combates contra jefes de Cataclysm.",
        is_api: false,
        equivalents: &["cinematiccataclysm", "cinematic_cataclysm"],
    },
    KnownMod {
        id: "cinematic-zoom",
        name: "Cinematic Zoom",
        category: "Calidad de Vida",
        tags: &["Zoom", "Camara", "QoL"],
        description: "Zoom suave y configurable para enfocar objetivos a distancia con transiciones fluidas.",
        is_api: false,
        equivalents: &["cinematiczoom", "cinematic_zoom"],
    },
    KnownMod {
        id: "cinematic-respawn",
        name: "Cinematic Respawn",
        category: "Calidad de Vida",
        tags: &["Camara", "Respawn", "Cinematica"],
        description: "Sustituye la pantalla de muerte estatica por una transicion cinematica fluida al reaparecer.",
        is_api: false,
        equivalents: &["cinematicrespawn", "cinematic_respawn"],
    },
    KnownMod {
        id: "smooth-third-person-camera",
        name: "Smooth Third Person Camera",
        category: "Calidad de Vida",
        tags: &["Camara", "Suavizado", "QoL"],
        description: "Suaviza el movimiento y la transicion de la camara en tercera persona.",
        is_api: false,
        equivalents: &["smooththirdpersoncamera", "smooth_third_person_camera"],
    },
    KnownMod {
        id: "uncrafting-table",
        name: "Uncrafting Table",
        category: "Calidad de Vida",
        tags: &["Crafteo", "Recetas", "Desarmar", "QoL"],
        description: "Mesa de descrafteo que permite desarmar objetos y recuperar sus materiales y componentes originales.",
        is_api: false,
        equivalents: &["uncraftingtable", "uncraftingtable76", "uncrafting_table"],
    },
    KnownMod {
        id: "immersive-ui",
        name: "ImmersiveUI",
        category: "Interfaz",
        tags: &["UI", "Interfaz", "Animaciones"],
        description: "Moderniza y embellece la interfaz de usuario de Minecraft con efectos y animaciones inmersivas.",
        is_api: false,
        equivalents: &["immersiveui", "immersive_ui"],
    },
    KnownMod {
        id: "item-borders",
        name: "Item Borders",
        category: "Interfaz",
        tags: &["UI", "Inventario", "Bordes", "Rareza"],
        description: "Añade bordes coloreados segun la rareza a los slots de items en el inventario y pantallas de cofres.",
        is_api: false,
        equivalents: &["itemborders", "item_borders"],
    },
    KnownMod {
        id: "smooth-swapping",
        name: "Smooth Swapping",
        category: "Interfaz",
        tags: &["UI", "Inventario", "Animaciones"],
        description: "Añade animaciones suaves al mover o intercambiar items dentro del inventario.",
        is_api: false,
        equivalents: &["smoothswapping", "smooth_swapping"],
    },
    KnownMod {
        id: "dungeon-crawl",
        name: "Dungeon Crawl",
        category: "Mundo y Generación",
        tags: &["Mazmorras", "Estructuras", "Subterraneo"],
        description: "Genera inmensas mazmorras subterraneas de varios niveles repletas de monstruos y tesoros.",
        is_api: false,
        equivalents: &["dungeoncrawl", "dungeon_crawl"],
    },
    KnownMod {
        id: "dungeons-and-taverns",
        name: "Dungeons and Taverns",
        category: "Mundo y Generación",
        tags: &["Mazmorras", "Tabernas", "Estructuras"],
        description: "Añade y rediseña decenas de estructuras, fortalezas, tabernas y mazmorras en el mundo.",
        is_api: false,
        equivalents: &["mr_dungeons_andtaverns", "dungeonsandtaverns", "dungeons_and_taverns"],
    },
    KnownMod {
        id: "moogs-voyager-structures",
        name: "Moog's Voyager Structures",
        category: "Mundo y Generación",
        tags: &["Estructuras", "Aventura", "Exploracion"],
        description: "Mas de 160 estructuras y templos unicos construidos con bloques vanilla integrados armoniosamente.",
        is_api: false,
        equivalents: &["mvs", "moogsvoyagerstructures", "moogs_voyager_structures", "moog-voyager-structures"],
    },
    KnownMod {
        id: "moogs-structure-lib",
        name: "Moog's Structure Lib",
        category: "APIs y Dependencias",
        tags: &["Estructuras", "Libreria", "Dependencias"],
        description: "Libreria de soporte requerida por los mods de estructuras de Moog.",
        is_api: true,
        equivalents: &[
            "moogs_structures",
            "moogs-structures",
            "moogsstructures",
            "moogsstructurelib",
            "moogs_structure_lib",
            "moog_structure_lib",
            "moogstructurelib",
        ],
    },
    KnownMod {
        id: "sparse-structures",
        name: "Sparse Structures",
        category: "Mundo y Generación",
        tags: &["Estructuras", "Generacion", "Distribucion"],
        description: "Espacia y distribuye la distancia entre estructuras para evitar acumulaciones excesivas en el mundo.",
        is_api: false,
        equivalents: &["sparsestructures", "sparse_structures"],
    },
    KnownMod {
        id: "structure-essentials",
        name: "Structure Essentials",
        category: "Mundo y Generación",
        tags: &["Estructuras", "Optimizacion", "Generacion"],
        description: "Herramientas y mejoras de rendimiento para la generacion fluida de estructuras.",
        is_api: false,
        equivalents: &["structureessentials", "structure_essentials"],
    },
    KnownMod {
        id: "explorify",
        name: "Explorify",
        category: "Mundo y Generación",
        tags: &["Estructuras", "Dungeons", "Exploracion"],
        description: "Añade decenas de estructuras y ruinas tematicas vanilla-friendly que recompensan la exploracion.",
        is_api: false,
        equivalents: &["explorify_v1", "explorify_v2"],
    },
    KnownMod {
        id: "better-village",
        name: "Better Village",
        category: "Mundo y Generación",
        tags: &["Aldeas", "Estructuras", "Aldeanos"],
        description: "Rediseño completo de las aldeas de Minecraft con edificios detallados y decoraciones unicas.",
        is_api: false,
        equivalents: &["bettervillage", "better_village"],
    },
    KnownMod {
        id: "repurposed-structures",
        name: "Repurposed Structures",
        category: "Mundo y Generación",
        tags: &["Estructuras", "Biomas", "Variantes"],
        description: "Crea variantes climaticas y regionales de templos, aldeas, fortalezas y puestos en todos los biomas.",
        is_api: false,
        equivalents: &["repurposedstructures", "repurposed_structures_forge"],
    },
    KnownMod {
        id: "minecolonies",
        name: "MineColonies",
        category: "Mundo y Generación",
        tags: &["Colonias", "Construccion", "Aldeanos"],
        description: "Permite fundar y liderar una colonia con aldeanos especializados que construyen y trabajan de forma autonoma.",
        is_api: false,
        equivalents: &[],
    },
    KnownMod {
        id: "fzzy-config",
        name: "Fzzy Config",
        category: "APIs y Dependencias",
        tags: &["Configuracion", "Libreria", "Dependencias", "GUI"],
        description: "Libreria y framework de configuracion avanzada dentro del juego para mods.",
        is_api: true,
        equivalents: &["fzzy_config", "fzzyconfig"],
    },
    KnownMod {
        id: "lithostitched",
        name: "Lithostitched",
        category: "APIs y Dependencias",
        tags: &["Worldgen", "Libreria", "Dependencias"],
        description: "Libreria técnica que amplía la configurabilidad y compatibilidad de la generación de mundo (worldgen).",
        is_api: true,
        equivalents: &["lithostitched", "lithostiched", "litho-stitched"],
    },
    KnownMod {
        id: "patchouli",
        name: "Patchouli",
        category: "APIs y Dependencias",
        tags: &["Guias", "Libros", "Libreria", "Dependencias"],
        description: "Libreria de documentación y creación de libros y guías ilustradas interactivas dentro del juego.",
        is_api: true,
        equivalents: &["patchouli", "parchouli"],
    },
    KnownMod {
        id: "grand-teleport",
        name: "Grand Teleport",
        category: "Calidad de Vida",
        tags: &["Teletransporte", "Viaje", "Cinematica", "Calidad de vida"],
        description: "Sistema de teletransporte cinemático y dinámico estilo GTA para viajar entre puntos y waypoints.",
        is_api: false,
        equivalents: &["gtalike_teleport", "grandteleport", "grand_teleport", "grand-teleport"],
    },
    KnownMod {
        id: "simple-voice-chat",
        name: "Simple Voice Chat",
        category: "Audio y Comunicación",
        tags: &["Voz", "Audio", "Microfono", "Proximidad"],
        description: "Chat de voz por proximidad en el juego con audio posicional 3D.",
        is_api: false,
        equivalents: &["voicechat", "simple_voice_chat", "simplevoicechat"],
    },
    KnownMod {
        id: "recipe-essentials",
        name: "Recipe Essentials",
        category: "Rendimiento",
        tags: &["Recetas", "Rendimiento", "Optimizacion"],
        description: "Optimiza y añade multihilo al sistema de recetas JSON para mejorar el arranque y sincronización.",
        is_api: false,
        equivalents: &["recipeessentials", "recipe_essentials", "recipe-essentials"],
    },
    KnownMod {
        id: "structure-essentials",
        name: "Structure Essentials",
        category: "Rendimiento",
        tags: &["Estructuras", "Rendimiento", "Optimizacion"],
        description: "Optimiza la generación y procesamiento de estructuras en el mundo para evitar tirones y lag.",
        is_api: false,
        equivalents: &["structureessentials", "structure_essentials", "structure-essentials"],
    },
];

/// Id / equivalent as these lookups compare them: lowercase, without '-' and '_'.
fn known_key(s: &str) -> String {
    s.chars().filter(|c| *c != '-' && *c != '_').collect::<String>().to_lowercase()
}

/// known_key of every KNOWN_MODS id and equivalent -> indices of the entries it belongs to.
/// Built once: scanning and comparing used to walk (and re-lowercase) the whole table for
/// every mod and every pair of mods.
fn known_index() -> &'static std::collections::HashMap<String, Vec<usize>> {
    static INDEX: std::sync::OnceLock<std::collections::HashMap<String, Vec<usize>>> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
        for (i, m) in KNOWN_MODS.iter().enumerate() {
            for key in std::iter::once(m.id).chain(m.equivalents.iter().copied()) {
                let entry = map.entry(known_key(key)).or_default();
                if entry.last() != Some(&i) {
                    entry.push(i);
                }
            }
        }
        map
    })
}

pub fn find_known_mod(slug: &str, raw_id: &str) -> Option<&'static KnownMod> {
    // First entry (in table order) whose id or an equivalent matches either value.
    let index = known_index();
    let by_slug = index.get(&slug.to_lowercase()).and_then(|v| v.first());
    let by_id = index.get(&known_key(raw_id)).and_then(|v| v.first());
    match (by_slug, by_id) {
        (Some(a), Some(b)) => Some(&KNOWN_MODS[*a.min(b)]),
        (Some(i), None) | (None, Some(i)) => Some(&KNOWN_MODS[*i]),
        (None, None) => None,
    }
}

pub fn are_equivalents(id_a: &str, id_b: &str) -> bool {
    let a_clean = known_key(id_a);
    let b_clean = known_key(id_b);

    if a_clean == b_clean {
        return true;
    }

    // Equivalent when some known mod lists both (as its id or among its equivalents).
    let index = known_index();
    match (index.get(&a_clean), index.get(&b_clean)) {
        (Some(ia), Some(ib)) => ia.iter().any(|i| ib.contains(i)),
        _ => false,
    }
}

pub fn infer_category_and_tags(name: &str, desc: &str, slug: &str) -> (&'static str, Vec<String>, bool) {
    let combined = format!("{} {} {}", name, desc, slug).to_lowercase();

    let is_api = combined.contains("api")
        || combined.contains("library")
        || combined.contains("libreria")
        || combined.contains("framework")
        || combined.contains("core")
        || combined.contains("cloth config")
        || combined.contains("architectury")
        || combined.contains("geckolib")
        || combined.contains("citadel")
        || combined.contains("balm")
        || combined.contains("bookshelf")
        || combined.contains("curios")
        || combined.contains("trinkets")
        || combined.contains("puzzleslib")
        || combined.contains("azurelib")
        || combined.contains("tctcore")
        || combined.contains("fzzy")
        || combined.contains("lithostitched")
        || combined.contains("lithostiched")
        || combined.contains("patchouli")
        || combined.contains("parchouli")
        || slug.contains("azurelib")
        || slug.contains("tctcore")
        || slug.contains("fzzy")
        || slug.contains("lithostitched")
        || slug.contains("patchouli")
        || slug.ends_with("lib")
        || slug.ends_with("api")
        || slug.ends_with("core");

    let mut tags = Vec::new();

    // Taxonomy aligned with Modrinth & CurseForge official tags
    let category = if combined.contains("optim")
        || combined.contains("perform")
        || combined.contains("fps")
        || combined.contains("cull")
        || combined.contains("memory")
        || combined.contains("multithread")
        || combined.contains("chunk loading")
        || combined.contains("fast")
        || combined.contains("leak")
        || combined.contains("easy mob spawn")
        || combined.contains("easy_mob_spawn")
        || combined.contains("easymobspawn")
        || combined.contains("modfixer")
        || combined.contains("mod_fixer")
        || combined.contains("mod-fixer")
        || combined.contains("chunky")
    {
        if combined.contains("fps") { tags.push("FPS".to_string()); }
        if combined.contains("chunk") { tags.push("Chunks".to_string()); }
        if combined.contains("memory") || combined.contains("ram") { tags.push("Memoria".to_string()); }
        if combined.contains("mob") { tags.push("Mobs".to_string()); }
        if combined.contains("fixer") { tags.push("Compatibilidad".to_string()); }
        "Rendimiento"
    } else if combined.contains("shader")
        || combined.contains("visual")
        || combined.contains("render")
        || combined.contains("graphic")
        || combined.contains("light")
        || combined.contains("texture")
        || combined.contains("model")
        || combined.contains("cosmetic")
        || combined.contains("animation")
        || combined.contains("particle")
        || combined.contains("fog")
        || combined.contains("sky")
        || combined.contains("camera")
        || combined.contains("zoom")
        || combined.contains("better third person")
        || combined.contains("betterthirdperson")
        || combined.contains("afk camera")
        || combined.contains("afkcam")
        || combined.contains("cinematic")
    {
        if combined.contains("shader") { tags.push("Shaders".to_string()); }
        if combined.contains("light") { tags.push("Iluminacion".to_string()); }
        if combined.contains("texture") { tags.push("Texturas".to_string()); }
        if combined.contains("camera") || combined.contains("zoom") { tags.push("Camara".to_string()); }
        "Gráficos, Cámara y Animaciones"
    } else if combined.contains("structure")
        || combined.contains("structur")
        || combined.contains("dungeon")
        || combined.contains("village")
        || combined.contains("town")
        || combined.contains("tower")
        || combined.contains("temple")
        || combined.contains("stronghold")
        || combined.contains("mineshaft")
        || combined.contains("outpost")
        || combined.contains("ruin")
        || combined.contains("fortress")
        || combined.contains("colony")
        || combined.contains("colonies")
        || combined.contains("explorify")
        || combined.contains("taverns")
        || combined.contains("worldgen")
        || combined.contains("world gen")
        || combined.contains("biome")
        || combined.contains("terrain")
        || combined.contains("dimension")
        || combined.contains("cave")
        || combined.contains("generation")
        || combined.contains("terralith")
        || combined.contains("tectonic")
    {
        if combined.contains("dungeon") { tags.push("Mazmorras".to_string()); }
        if combined.contains("village") || combined.contains("town") { tags.push("Aldeas".to_string()); }
        if combined.contains("biome") { tags.push("Biomas".to_string()); }
        if combined.contains("terrain") { tags.push("Terreno".to_string()); }
        if combined.contains("dimension") { tags.push("Dimensiones".to_string()); }
        tags.push("Mundo y Generación".to_string());
        "Mundo y Generación"
    } else if combined.contains("uncrafting table")
        || combined.contains("uncraftingtable")
        || combined.contains("uncrafting")
        || combined.contains("grand teleport")
        || combined.contains("grandteleport")
        || combined.contains("grand_teleport")
        || combined.contains("gtalike_teleport")
        || combined.contains("teleport")
        || combined.contains("recipe")
        || combined.contains("inventory")
        || combined.contains("convenien")
        || combined.contains("quality of life")
        || combined.contains("qol")
        || combined.contains("sort")
        || combined.contains("storage")
        || combined.contains("backpack")
        || combined.contains("trash")
        || combined.contains("carryon")
        || combined.contains("seamlesssleep")
        || combined.contains("bug fix")
        || combined.contains("bugfix")
        || combined.contains("fix")
        || combined.contains("patch")
        || combined.contains("compat")
    {
        if combined.contains("teleport") { tags.push("Teletransporte".to_string()); }
        if combined.contains("recipe") || combined.contains("crafteo") || combined.contains("uncraft") { tags.push("Recetas".to_string()); }
        if combined.contains("inventory") || combined.contains("storage") { tags.push("Inventario".to_string()); }
        tags.push("Calidad de Vida".to_string());
        "Calidad de Vida"
    } else if combined.contains("map")
        || combined.contains("hud")
        || combined.contains("gui")
        || combined.contains("interface")
        || combined.contains("cursor")
        || combined.contains("crosshair")
        || combined.contains("better looting")
        || combined.contains("better_looting")
        || combined.contains("betterlooting")
        || combined.contains("immersive ui")
        || combined.contains("immersiveui")
        || combined.contains("immersive_ui")
        || combined.contains("item borders")
        || combined.contains("itemborders")
        || combined.contains("item_borders")
        || combined.contains("smooth swapping")
        || combined.contains("smoothswapping")
        || combined.contains("smooth_swapping")
        || combined.contains("smoothgui")
        || combined.contains("healthindicators")
        || combined.contains("highlighter")
        || combined.contains("tooltip")
        || (!is_api && (combined.contains("menu") || combined.contains("screen") || combined.contains("information")) && !combined.contains("tctcore") && !combined.contains("easy mob spawn") && !combined.contains("modfixer"))
    {
        if combined.contains("map") { tags.push("Mapa".to_string()); }
        if combined.contains("hud") { tags.push("HUD".to_string()); }
        if combined.contains("loot") { tags.push("Loot".to_string()); }
        if combined.contains("border") { tags.push("Bordes".to_string()); }
        tags.push("Interfaz".to_string());
        "Interfaz"
    } else if is_api {
        tags.push("Libreria".to_string());
        tags.push("APIs y Dependencias".to_string());
        "APIs y Dependencias"
    } else if combined.contains("voicechat")
        || combined.contains("voice chat")
        || combined.contains("sound")
        || combined.contains("audio")
        || combined.contains("music")
        || combined.contains("acoustic")
        || combined.contains("multiplayer")
        || combined.contains("server utility")
        || combined.contains("essential")
        || combined.contains("e4mc")
    {
        if combined.contains("voice") { tags.push("Voz".to_string()); }
        tags.push("Audio y Comunicación".to_string());
        "Audio y Comunicación"
    } else if combined.contains("cataclysm") {
        tags.push("Contenido".to_string());
        tags.push("Jefes".to_string());
        tags.push("Aventura".to_string());
        "Contenido"
    } else if combined.contains("combat")
        || combined.contains("weapon")
        || combined.contains("armor")
        || combined.contains("shield")
        || combined.contains("sword")
        || combined.contains("gun")
        || combined.contains("bow")
    {
        tags.push("Combate".to_string());
        "Combate"
    } else if combined.contains("build")
        || combined.contains("furniture")
        || combined.contains("decor")
        || combined.contains("chisel")
        || combined.contains("block")
    {
        tags.push("Decoracion".to_string());
        "Construcción y Decoración"
    } else {
        "Contenido"
    };

    (canonical_category(category), tags, is_api)
}

pub fn canonical_category(cat: &str) -> &'static str {
    let c = cat.trim().to_lowercase();
    if c.contains("rendimiento") || c.contains("perform") || c.contains("fps") || c.contains("optimi") {
        "Rendimiento"
    } else if c.contains("mundo") || c.contains("genera") || c.contains("estructur") || c.contains("world") || c.contains("biome") || c.contains("dungeon") {
        "Mundo y Generación"
    } else if c.contains("vida") || c.contains("qol") || c.contains("utilidad") || c.contains("fix") || c.contains("convenien") {
        "Calidad de Vida"
    } else if c.contains("interfaz") || c == "ui" || c.contains("ui ") || c.contains(" ui") || c.contains("map") || c.contains("hud") || c.contains("gui") {
        "Interfaz"
    } else if c.contains("gráfico") || c.contains("grafico") || c.contains("cámara") || c.contains("camara") || c.contains("animaci") || c.contains("shader") || c.contains("visual") {
        "Gráficos, Cámara y Animaciones"
    } else if c.contains("combate") || c.contains("combat") || c.contains("weapon") || c.contains("arma") {
        "Combate"
    } else if c.contains("construc") || c.contains("decor") || c.contains("build") || c.contains("mueble") {
        "Construcción y Decoración"
    } else if c.contains("audio") || c.contains("comunicaci") || c.contains("voz") || c.contains("voice") || c.contains("sonido") || c.contains("sound") || c.contains("multiplayer") {
        "Audio y Comunicación"
    } else if c.contains("api") || c.contains("dependencia") || c.contains("lib") || c.contains("librer") {
        "APIs y Dependencias"
    } else {
        "Contenido"
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DetailedModPayload {
    pub mod_id: String,
    pub name: String,
    pub category: String,
    pub description_es: String,
    pub features: Vec<String>,
    pub commands: Vec<String>,
    pub wiki_url: String,
    pub source_url: String,
    pub config_tip: String,
}

pub fn translate_to_spanish(desc: &str) -> String {
    let t = desc.trim();
    if t.is_empty() {
        return "Sin descripción disponible.".to_string();
    }

    // Strip Minecraft formatting/color codes (§l, §r, etc.)
    let unformatted = if let Ok(re) = crate::scanner::cached_regex(r"§[0-9a-fk-or]") {
        re.replace_all(t, "").to_string()
    } else {
        t.to_string()
    };

    let trimmed = unformatted.trim();
    if trimmed.is_empty() {
        return "Sin descripción disponible.".to_string();
    }

    let lower = trimmed.to_lowercase();

    // Direct match for music packs / URLs or Cataclysm links
    if lower.contains("cataclysmic-tunes") || trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return "Añade mazmorras colosales, jefes desafiantes y armas míticas.".to_string();
    }

    // Direct match for weaponmaster promotional text
    if lower.contains("hotbar items will be visible on your character") {
        return "Muestra los objetos de la barra rápida, escudos y armas directamente en el modelo del jugador.".to_string();
    }

    // Specific known mod descriptions from actual modpack jars
    let exact_matches = [
        ("The Minecraft Bosses", "Añade nuevos jefes desafiantes y combates épicos en Minecraft."),
        ("Adds one-time intro cutscenes for some L_Enders_Cataclysm bosses", "Añade cinemáticas de presentación para los jefes de Cataclysm."),
        ("Cinematic respawn experience for Minecraft.", "Experiencia cinematográfica inmersiva al reaparecer tras morir."),
        ("Build your own bakery, bake the finest pastries, delicious bread, and sweet cakes!", "Construye tu propia panadería, hornea bollería, pan delicioso y pasteles."),
        ("Build your own brewery, brew finest beers and drinks!", "Construye tu propia cervecería, fermenta y elabora cervezas y bebidas."),
        ("Build your own winery, grow grapes and craft fine wine!", "Construye tu propia vinatería, cultiva uvas y produce vino artesanal."),
        ("A collection of beach themed furniture, food and drinks!", "Colección de muebles, comida y bebidas temáticas de playa."),
        ("A collection of herbal teas, drinks and herbs!", "Colección de infusiones herbales, tés medicinales y bebidas naturales."),
        ("Farming and cooking, decorative blocks and much more!", "Agricultura y cocina, bloques decorativos y mucho contenido culinario."),
        ("Funni block placement animations", "Animaciones visuales dinámicas y divertidas al colocar bloques."),
        ("First-person animations. Special thanks to Godku for the support.", "Animaciones fluidas en primera persona para el personaje."),
        ("Shows the last used tool on the player's back. Purely aesthetic", "Muestra la última herramienta usada en la espalda del jugador (estético)."),
        ("Shows the last used tool on the player's back", "Muestra la última herramienta usada en la espalda del jugador (estético)."),
        ("Replaces standard advancement toasts with fancy plaques.", "Reemplaza las notificaciones de logros estándar por placas decorativas elegantes."),
        ("Cinematic zoom with animated crosshair, adjustable sounds and in-game editor", "Zoom cinematográfico con retícula animada, sonidos ajustables y editor dentro del juego."),
        ("Adds independent camera rotation for third-person view.", "Añade rotación de cámara libre e independiente en tercera persona."),
        ("A smooth freecam mod. Press F6 to fly the camera freely, press F6 again to smoothly return to gameplay.", "Cámara libre fluida. Pulsa F6 para volar libremente y volver al juego."),
        ("Highlights newly picked-up items in your inventory.", "Resalta visualmente los objetos recién recogidos en el inventario."),
        ("Makes your UI flashy :D", "Mejora estética y estilizada para la interfaz de usuario."),
        ("Interactive moving block structures with physics.", "Estructuras de bloques móviles interactivos con simulación de físicas."),
        ("Ragdoll physics API for Sable. Provides the core ragdoll system and API for addon mods.", "API de físicas ragdoll para Sable y mods complementarios."),
        ("This mod adds patches and features to Sable-Player-Ragdoll and Sable-Ragdoll-Corpse.", "Parches y mejoras para el sistema de ragdoll del jugador."),
        ("Replaces the vanilla death animation with a verlet-integration ragdoll that reacts to the direction and force of the killing blow.", "Reemplaza la animación de muerte por físicas ragdoll según la dirección y fuerza del impacto."),
        ("Players and mobs ragdoll in reaction to the world around them! Crashes, explosions, damage, and more send them tumbling.", "Criaturas y jugadores reaccionan con físicas ragdoll a impactos, caídas y explosiones."),
        ("Rethinks the way players sleep, adding a smooth animation and visual improvements with minimal server impact. Now available for Forge, NeoForge and Fabric! *There are configs in this mod that can be tweaked*", "Mejora la forma de dormir con animaciones suaves y mejoras visuales sin lag."),
        ("Adding many new subtle details through particles and a few sounds", "Añade detalles sutiles e inmersivos mediante partículas y efectos de sonido."),
        ("Particle optimizations. Particle culling, rendering optimization, and configurable disabling of particles.", "Optimización profunda de partículas, culling y opciones de personalización."),
        ("Particular is a mod that enhances Minecraft", "Mejora las partículas del juego haciéndolas más vistosas y atmosféricas."),
        ("Gives rare items a fancy tooltip and adds additional tooltip configuration options.", "Otorga cuadros de información (tooltips) elegantes a objetos raros con opciones configurables."),
        ("Modern data driven tooltip renderer for Simply mods. Optionally available for other vanilla and modded items.", "Renderizador moderno de tooltips basado en datos."),
        ("Speed up and optimize immediate mode rendering in Minecraft", "Acelera y optimiza el renderizado de modo inmediato en Minecraft."),
        ("Optimization mod that focuses on things other than rendering", "Mod de optimización centrado en la lógica del juego y cálculo interno."),
        ("Egregious, yet effective performance improvements for modern Minecraft", "Mejoras masivas de rendimiento, tiempo de arranque y memoria para Minecraft moderno."),
        ("Small adjustments and improvements to AI logic focusing on optimizations for increased server performance", "Ajustes y optimizaciones en la IA de criaturas para mejorar el rendimiento del servidor."),
        ("Speed up your world loading by removing spawn chunks.", "Acelera la carga del mundo eliminando los spawn chunks innecesarios."),
        ("Pre-generates chunks, quickly, efficiently, and safely", "Pre-genera chunks del mundo de forma rápida, eficiente y segura para evitar lag."),
        ("Furnacing things faster, performance-wise", "Optimiza internamente la lógica de hornos para reducir el lag."),
        ("Supercharged item frames! Less lag, better performance, and now customizable", "Marcos de objetos optimizados: reduce el lag y mejora el rendimiento drásticamente."),
        ("Introduces multithreading to the json recipe system.", "Añade multihilo al sistema de recetas JSON para arranque instantáneo."),
        ("Speeding up crafting operations - massively.", "Acelera masivamente el cálculo y búsqueda de recetas en mesas de crafteo."),
        ("This mod lets you return to the Title Screen early while your world is still saving", "Permite volver al menú principal de inmediato mientras el mundo se guarda en segundo plano."),
        ("Off-thread event polling", "Procesamiento de eventos fuera del hilo principal para mayor fluidez."),
        ("A lightweight entity-tick-scheduler for doubling your TPS.", "Programador ligero de ticks de entidades para duplicar los TPS."),
        ("Improve item performance and detects, reports and removes too many stacked items.", "Mejora el rendimiento de objetos en el suelo y previene aglomeraciones con lag."),
        ("spark is a performance profiling plugin/mod for Minecraft clients, servers and proxies.", "Herramienta avanzada para diagnosticar y perfilar causas de lag y consumo de CPU."),
        ("A NeoForge mod that aims to optimize the minecraft server.", "Optimización integral de servidor para NeoForge."),
        ("Sodium is a powerful rendering engine for Minecraft which improves frame rates and reduces lag spikes.", "Motor de renderizado avanzado que multiplica los FPS y elimina tirones."),
        ("Provides powerful network optimization capabilities for all mainstream systems.", "Optimización potente de red y paquetes de datos para todo tipo de conexiones."),
        ("Lightweight mod which solves connection and packet size issues.", "Soluciona problemas de desconexión y límites de tamaño de paquetes."),
        ("Add a Clearlag to your world or your server to improve your performance in large structures with many items", "Elimina entidades sobrantes para mejorar el rendimiento en estructuras grandes."),
        ("A Forge mod designed to improve the chunk performance of Minecraft.", "Optimización profunda multihilo para la carga y generación de chunks."),
        ("An efficient and non-locational redstone dust implementation.", "Implementación de polvo de redstone ultra rápida y sin lag."),
        ("This mod fixes memory leaks when needed!", "Corrige fugas de memoria automáticamente cuando es necesario."),
        ("This mod fixes memory leaks when needed", "Corrige fugas de memoria automáticamente cuando es necesario."),
        ("Cull leaves while looking hot!", "Oculta caras internas de hojas para aumentar los FPS manteniendo una gran estética."),
        ("Features that shouldn't be in Sodium", "Opciones gráficas avanzadas y personalización adicional para Sodium/Embeddium."),
        ("Features that shouldn", "Opciones gráficas avanzadas y personalización adicional para Sodium/Embeddium."),
        ("Bridges big and small across rivers.", "Añade puentes grandes y pequeños para cruzar ríos."),
        ("Bridges big and small across rivers", "Añade puentes grandes y pequeños para cruzar ríos."),
        ("A extendable and data-driven Accessory Mod for Minecraft.", "Mod modular de accesorios y ranuras de equipamiento."),
        ("An extendable and data-driven Accessory Mod for Minecraft.", "Mod modular de accesorios y ranuras de equipamiento."),
        ("A flexible and expandable accessory/equipment API for users and developers.", "API y sistema flexible de ranuras de accesorios y equipamiento."),
        ("Easy, spectacular and fun melee combat system from Minecraft Dungeons.", "Sistema de combate cuerpo a cuerpo fluido y espectacular de Minecraft Dungeons."),
        ("Better Flight aims to improve vanilla Elytra mechanics, making them more fun to use in a balanced way that reduces the need for firework rockets and gunpowder.", "Mejora las mecánicas de vuelo con elytras haciéndolas más equilibradas y divertidas sin depender de cohetes."),
        ("Better Flight aims to improve vanilla Elytra mechanics", "Mejora las mecánicas de vuelo con elytras haciéndolas más equilibradas y divertidas."),
        ("Small mod that replaces one biome with another, without affecting other parts of the generation.", "Reemplaza un bioma por otro sin alterar el resto de la generación del mundo."),
        ("Adds over 50 new biomes, blocks, and more!", "Añade más de 50 biomas nuevos, bloques y mucho contenido."),
        ("Botania is a mod that adds nature magic to Minecraft.", "Extenso mod de magia natural y automatización basada en flores y maná."),
        ("Carry On is a simple mod that improves game interaction by allowing players to pick up, carry, and place single block Tile Entities using only their empty hands.", "Permite levantar, transportar y colocar cofres, hornos y bloques con las manos vacías."),
        ("Connected Glass adds new types of glass with connecting textures!", "Añade nuevos cristales con texturas continuas y conectadas."),
        ("Connect your fences with a decorative chain!", "Permite conectar vallas con cadenas decorativas colgantes."),
        ("Shows a GUI after Minecraft crashes, immediately showing all affected logs, crash reports, or hs_err files. Provides a one-click solution to upload them, copy the link, and perform other actions for easier reporting, debugging, and troubleshooting.", "Muestra una interfaz tras un cierre inesperado con acceso directo a informes y registros."),
        ("Microsoft flight simulator for Minecraft elytras.", "Físicas avanzadas de vuelo realista estilo simulador para las elytras."),
        ("A Structure Datapack adding dungeons, taverns and other structures to find while you explore the world.", "Añade mazmorras, tabernas y estructuras para explorar por el mundo."),
        ("Clumps xp orbs together.", "Agrupa los orbes de experiencia en uno solo para reducir el lag."),
        ("A simplistic, vanilla-friendly collection of new structures.", "Colección de estructuras nuevas respetuosas con el estilo vanilla."),
        ("Adds smaller cut versions of various solid blocks for Hammer Multipart.", "Añade versiones cortadas de bloques para construcción detallada."),
        ("Simple and powerful library to place multiple things in one block space.", "Librería para colocar múltiples elementos en un mismo bloque."),
        ("Simple damage indicator mod that shows a floating health bar (or number) above entities.", "Indicador de daño con barra de vida flotante sobre las entidades."),
        ("Lightweight AFK mod that makes idle time cinematic, optionally hides HUD", "Mod ligero que activa una cámara cinematográfica al estar ausente."),
        ("This mod will show you your inventory while playing. Enjoy!", "Muestra tu inventario y equipamiento en pantalla mientras juegas."),
        ("Mod that lets you place items on any block side", "Permite colocar objetos decorativamente sobre cualquier cara de un bloque."),
        ("Iris is a modern shaders mod for Minecraft intended to be compatible with existing OptiFine shader packs.", "Mod de shaders moderno compatible con paquetes de shaders de OptiFine."),
        ("JEI is an item and recipe viewing mod for Minecraft, built from the ground up for stability and performance.", "Visor esencial de objetos, recetas y usos, optimizado para máxima estabilidad."),
        ("Fixes the infamous MC-17876 bug. This bug would cause players with above 20 health to be reset when the restart the game.", "Corrige el bug que reiniciaba la vida adicional del jugador al reiniciar."),
        ("MineColonies is a colony simulator within Minecraft! There are numerous types of workers, and each worker has its own type of hut, letting you build a busy and vibrant village/town/city/colony. There is no coded limit to the number of NPC", "Simulador de colonias: crea una aldea viva con trabajadores, aldeanos y edificios."),
        ("A mod that enhances the inventory management by adding various additional functions to the usual mouse buttons.", "Mejora la gestión de inventario con funciones rápidas y atajos de ratón."),
        ("Piston unlimited adds the multipiston block which allows you to move any block with a handy GUI into any direction!", "Permite mover cualquier bloque en cualquier dirección mediante pistones avanzados."),
        ("Enhances the overworld by adding new wildlife with unique behavior!", "Mejora la superficie añadiendo fauna salvaje con comportamientos realistas."),
        ("Search for a biome and get information about it.", "Brújula especial para buscar y localizar biomas en el mundo."),
        ("Fix the Bees!", "Corrige problemas de IA y comportamiento en las abejas."),
        ("A lightweight library to easily animate the player", "Librería ligera para animaciones del jugador."),
        ("A library that allows mods to animate the player, in a way that doesn't conflict", "Librería para animar al jugador sin conflictos entre mods."),
        ("No more recipe conflicts! Adds an option to choose the crafting result if more than one is available.", "¡Sin conflictos de crafteo! Permite elegir el resultado si varias recetas coinciden."),
        ("A stack of new biomes spread across the Overworld and Nether!", "Gran variedad de biomas nuevos para la superficie y el Nether."),
        ("Allows you to sit on slabs and stairs.", "Te permite sentarte cómodamente en losas y escaleras."),
        ("A highly configurable sound overhaul mod that adds new sound effects while improving vanilla sounds too.", "Mejora integral de sonidos ambientales, bloques y pasos en el juego."),
        ("Small mod that Increases the size limit of the vanilla structure blocks Increases the size limit of the structure block from 48x48x48 to 127x127x127", "Aumenta el límite de tamaño de los bloques de estructura a 127x127x127."),
        ("Essential Utilities for structures", "Utilidades esenciales para la generación de estructuras."),
        ("An astonishing way to copy & paste structures! Allows you to: - save structures of your choice into a file of small size - load them back and place them anywhere anytime - create various shapes using math formulas - and many other nice things a great builder would love to have!", "Permite copiar, guardar, cargar y pegar estructuras fácilmente en el mundo."),
        ("Many functional and useful Vanilla+ blocks", "Gran variedad de bloques decorativos y funcionales estilo Vanilla+."),
        ("Gambling-style item upgrader: risk an item for a shot at a better one.", "Mejora de objetos por riesgo: arriesga un objeto por una versión superior."),
        ("Structure mod that brings an exciting twist to Minecraft by introducing hostile and unfriendly pillages, providing a thrilling contrast to the peaceful villages.", "Añade campamentos y estructuras de saqueadores hostiles al mundo."),
        ("In this mod I seek to re-create the vanilla biomes in a way that makes a more realistic and atmospheric world.", "Recrea los biomas vanilla haciéndolos más realistas y atmosféricos."),
        ("Server-authoritative Minecraft casino machines with selectable wager currencies, clear payouts, and animated games.", "Máquinas recreativas animadas con apuestas configurables y premios."),
        ("New, original, engaging, and aesthetic mobs for Minecraft.", "Criaturas originales, dinámicas y estéticas con mecánicas únicas."),
        ("New, original, engaging, and aesthetic mobs for Minecraft", "Criaturas originales, dinámicas y estéticas para Minecraft."),
        ("Performance focused compatibility tweaks for Alex", "Ajustes de compatibilidad y rendimiento para Alex's Mobs."),
        ("A library mod providing Attributes and related things.", "Librería base que gestiona atributos adicionales y compatibilidad."),
        ("A library mod providing Attributes and related things", "Librería base que gestiona atributos adicionales y compatibilidad."),
        ("A library for building other mods.", "Librería base requerida para el funcionamiento de otros mods."),
        ("A library for building other mods", "Librería base requerida para el funcionamiento de otros mods."),
        ("Loader independent library mod for frikinjay's mods", "Librería multi-loader requerida por mods complementarios."),
        ("Loader independent library mod for frikinjay", "Librería multi-loader requerida por mods complementarios."),
        ("Carbon Config is IC2Classics Config Implementation made open source", "Librería de código abierto para gestión de archivos de configuración."),
        ("Library with handy utilities.", "Librería auxiliar con utilidades prácticas para mods."),
        ("A simple but powerful coremod.", "Librería núcleo simple y potente para renderizado y utilidades."),
        ("A library mod aimed at abstracting mod loaders and providing various utilities.", "Librería de abstracción entre cargadores de mods."),
        ("A library mod aimed at abstracting mod loaders", "Librería de abstracción entre cargadores de mods."),
        ("HammerLib is an essential tool for developing mods easily. Packed with a lot of features!", "Librería de desarrollo repleta de herramientas y utilidades."),
        ("A library containing events, helpers, and utilities to make modding easier", "Librería con eventos y utilidades auxiliares."),
        ("An API for config screens.", "API y librería para pantallas de configuración de mods."),
        ("Shared code for my mod.", "Código compartido y utilidades para mods del autor."),
        ("A library mod with new configurability and compatibility enhancements for worldgen", "Librería con mejoras de configuración y compatibilidad para generación de mundo."),
        ("A small library with many cool unique features, from dynamic asset generation to dynamic villager schedules, data driven trades and Map Markers, simple multi loader utilities and more", "Librería con soporte para aldeanos dinámicos, recursos y compatibilidad."),
        ("A library mod which provides various utilities for IMB11", "Librería con utilidades para mods de IMB11."),
        ("Bringing Prickle, the JSON based configuration file format to Minecraft.", "Soporte para formato de configuración JSON Prickle en Minecraft."),
        ("A library for creating config files for your mod across multiple platforms.", "Librería multiplataforma para crear archivos de configuración."),
        ("A library / shared functionality mod for Sophisticated Storage and Backpacks", "Librería de funcionalidad compartida para Sophisticated Storage y Backpacks."),
        ("YetAnotherConfigLib (yacl) is just that. A builder-based configuration library for Minecraft.", "Librería moderna para crear interfaces gráficas de configuración."),
        ("Java Minecraft mod using Bedrock models to create custom models and animations for items, blocks, entities", "Permite modelos y animaciones avanzadas de Bedrock en Minecraft Java."),
        ("Java Minecraft mod using Bedrock models to create custom models and animations", "Permite modelos y animaciones avanzadas de Bedrock en Minecraft Java."),
        ("A shared code library for many of Alexthe666", "Librería de código compartido para mods de Alexthe666."),
        ("Supported entities and items will emit light by using the light block added in 21w13a", "Iluminación dinámica: las antorchas, linternas y objetos emiten luz en tiempo real."),
        ("Supported entities and items will emit light", "Iluminación dinámica: antorchas y objetos sostenidos emiten luz."),

        // Screenshot specific mods & jars
        ("asynchronous logging and efficient filtering", "Registro asíncrono y filtrado eficiente de consola para optimizar el rendimiento y mantener los registros limpios."),
        ("optimizes chunk sending", "Optimiza y equilibra el envío de paquetes de chunks en la red para reducir el lag."),
        ("balances chunk network load", "Equilibra la carga de red en la transmisión de chunks."),
        ("supercharged item frames", "¡Marcos de objetos optimizados! Menor lag, gran rendimiento y opciones de visibilidad."),
        ("this mod makes improvements to noise gen", "Optimiza y acelera el generador de ruido matemático para terrenos."),
        ("improvements to noise gen", "Optimiza y acelera el generador de ruido matemático para terrenos."),
        ("fastasyncworldsave mod", "Guarda el mundo de forma asíncrona en segundo plano para evitar tirones y congelamientos."),
        ("fastasyncworldsave", "Guarda el mundo de forma asíncrona en segundo plano para evitar tirones y lag."),
        ("makes chests use static models instead of bers", "Convierte los cofres en modelos estáticos optimizados en lugar de entidades complejas, eliminando lag."),
        ("fastchest", "Convierte los cofres en modelos estáticos optimizados en lugar de entidades complejas, eliminando lag."),
        ("improves the speed of iris", "Optimiza la velocidad y el cálculo de sombras en el motor de shaders Iris."),
        ("minecraft mod that distributes hud updates over multiple frames", "Distribuye las actualizaciones del HUD a lo largo de varios fotogramas para maximizar el rendimiento."),
        ("distributes hud updates over multiple frames", "Distribuye las actualizaciones del HUD a lo largo de varios fotogramas para maximizar el rendimiento."),
        ("fixes some gpu memory leaks", "Corrige fugas de memoria en la tarjeta gráfica (GPU)."),
        ("gpu memory leaks", "Corrige fugas de memoria en la tarjeta gráfica (GPU)."),
        ("gputape", "Corrige fallos y fugas de memoria en búferes de la tarjeta gráfica (GPU)."),
        ("fork of c2me", "Mod de alto rendimiento diseñado para acelerar la carga y generación de chunks (Fork de C2ME)."),
        ("harichunk", "Mod de alto rendimiento diseñado para acelerar la carga y generación de chunks (Fork de C2ME)."),
        ("a high-performance mod designed to improve chunk performance", "Mod de alto rendimiento diseñado para acelerar la carga y generación de chunks."),
        ("improve item performance and detects, reports and removes too many stacked items", "Mejora el rendimiento de objetos en el suelo y previene aglomeraciones con lag."),
        ("improve item performance and detects", "Mejora el rendimiento de objetos en el suelo y previene aglomeraciones con lag."),
        ("improve item performance", "Mejora el rendimiento de objetos en el suelo y previene aglomeraciones con lag."),
        ("reimplementation of chunksavingfix", "Evita errores y salientes rotos en árboles al guardar chunks limítrofes."),
        ("yeet the laggy reversed dns lookup", "Elimina la búsqueda DNS inversa lenta para acelerar el ping de servidores."),
        ("makes paintings way faster by turning them into blocks", "Acelera el renderizado de cuadros convirtiéndolos en bloques optimizados."),
        ("client-side gpu-friendly performance companion", "Optimización inteligente de GPU que regula dinámicamente entidades lejanas y marcos de objetos para maximizar FPS."),
        ("gpushift", "Optimización inteligente de GPU que regula dinámicamente entidades lejanas y marcos de objetos para maximizar FPS."),
        ("unloads chunks a give time after the last visiting player logs off", "Descarga chunks inactivos tras un tiempo para liberar memoria RAM."),
        ("limited chunks mod", "Descarga chunks inactivos tras un tiempo para liberar memoria RAM."),
        ("spreads chunk saves over time smoothly", "Distribuye el guardado de chunks progresivamente en el tiempo para evitar caídas de FPS."),
        ("smoothchunk", "Distribuye el guardado de chunks progresivamente en el tiempo para evitar caídas de FPS."),
        ("fast vertex transform and caching using compute shader", "Transformación rápida de vértices y caché acelerada por GPU para renderizado ultra veloz."),
        ("fast vertex transform", "Transformación rápida de vértices y caché acelerada por GPU para renderizado ultra veloz."),
        ("acceleratedrendering", "Transformación rápida de vértices y caché acelerada por GPU para renderizado ultra veloz."),
        ("a library to facilitate registration between mods, in particular macaw", "Librería auxiliar para facilitar el registro de bloques y compatibilidad en mods de Macaw."),
        ("displaying advanced information about loot tables and trades", "Plugin para mostrar información detallada de tablas de botín y comercios de aldeanos."),
        ("client-side mod that adds a cinematic camera when the player is afk", "Mod de cliente que añade una cámara cinematográfica cuando el jugador está ausente (AFK)."),
        ("a intermediary api aimed to ease developing multiplatform mods", "API intermediaria para facilitar el desarrollo y compatibilidad de mods multiplataforma."),
        ("adds various new powerful uncraftable items to make exploration a bit more interesting", "Añade artefactos poderosos no fabricables para enriquecer la exploración."),
        ("extends the maximum attribute ranges to allow for higher values", "Extiende los rangos máximos de atributos del juego permitiendo valores superiores."),
        ("abstraction layer (but not really) for blay", "Librería base de abstracción requerida para mods de Blay."),
        ("tired of being a walking vacuum? try rpg-style immersive loot management", "Gestión de botín inmersiva estilo RPG para no absorber ítems automáticamente."),
        ("this mod improves the vanilla minecraft villages", "Mejora y rediseña por completo las aldeas vanilla haciéndolas más variadas y vivas."),
        ("edit placed blocks with persistent spatial transforms", "Permite rotar, escalar y transformar bloques colocados de forma persistente."),
        ("ui library mod, use ctrl+alt+shift+x to open a test-ui", "Librería base de interfaces de usuario para mods de MineColonies."),
        ("a complete improvement to the overworld's biomes", "Mejora completa y rediseño de biomas de la superficie."),
        ("bookshelf is a library mod that provides code, frameworks, and utilities", "Librería base que proporciona código, utilidades y funciones comunes para otros mods."),
        ("an api to provide developers access to the elytra", "API que proporciona acceso y control a mecánicas de las elytras."),
        ("adds a samurai tower along with samurais", "Añade torres de samuráis, armas y guerreros samurái en el mundo."),
        ("makes sodium a tasteful experience", "Mejora visual y opciones adicionales de compatibilidad para Sodium."),
        ("clientcrafting mod", "Permite calcular recetas y crafteos directamente del lado del cliente para eliminar lag."),
        ("library mod which allows configuration file management and processing", "Librería para procesamiento y gestión de archivos de configuración."),
        ("adds the ability to search for keybinds using their name", "Añade una barra de búsqueda para encontrar teclas y atajos rápidamente en el menú de controles."),
        ("it utils", "Librería auxiliar con utilidades para mods de clima y criaturas."),
        ("to fix a server crash exploit", "Corrige vulnerabilidades y exploits que causan caídas inesperadas del servidor."),
        ("a library mod for easy structure config and runtime datapacks", "Librería para configuración de estructuras y carga de datapacks en tiempo de ejecución."),
        ("cupboard mod providing utilities", "Librería auxiliar Cupboard que proporciona utilidades de rendimiento y configuración."),
        ("cupboard library for utilities and configs", "Librería auxiliar Cupboard que proporciona utilidades de rendimiento y configuración."),
        ("allows you to play minecraft in a borderless fullscreen window", "Permite jugar a Minecraft en modo ventana sin bordes (pantalla completa sin marcos)."),
        ("featuring new blocks, items, armor, and hidden mysteries to complement the deep dark", "Expansión para las profundidades (Deep Dark) con nuevos bloques, armaduras, armas y dimensiones."),
        ("a way for modpacks to ship a default (key) configuration", "Permite a los modpacks incluir una configuración de teclas por defecto sin sobreescribir la del jugador."),
        ("home decorations made easy", "Mobiliario y decoración para el hogar fabricable con múltiples materiales."),
        ("dungeon crawl, a spiritual successor to roguelike dungeons", "Genera extensas mazmorras subterráneas multinivel con tesoros y desafíos estilo roguelike."),
        ("mob spawn control utility with in-game menu", "Herramienta con menú en el juego para controlar y limitar la aparición de criaturas."),
        ("makes building easier by providing tools like mirrors, arrays, build modes", "Herramientas de construcción rápida: modos espejo, líneas, esferas y aleatorizador de bloques."),
        ("adds cape-like physics to the elytra when worn by the player", "Añade físicas suaves estilo capa a las elytras cuando el jugador las lleva puestas."),
        ("adds an accessory slot for the elytra, so you can fly and wear chest armor", "Añade una ranura exclusiva para las elytras, permitiendo volar y llevar pechera al mismo tiempo."),
        ("embeddium is a fork of rubidium, a fork of sodium", "Port de Sodium para Forge y NeoForge con optimizaciones de renderizado y máxima compatibilidad."),
        ("adds descriptions of enchantments to their tooltip", "Muestra una descripción detallada del efecto de cada encantamiento en su cuadro informativo."),
        ("adds descriptions of enchantment effects", "Muestra una descripción detallada del efecto de cada encantamiento en su cuadro informativo."),
        ("adding more enemies to minecraft", "Añade una amplia variedad de nuevos enemigos y criaturas hostiles al juego."),
        ("custom entity model (cem) resource packs", "Permite modelos de entidad personalizados (CEM) estilo OptiFine en Minecraft."),
        ("adds support for resource-pack driven features for entity textures including some optifine features", "Soporte completo para texturas emisivas, animadas y variantes de criaturas estilo OptiFine."),
        ("this mod uses async path-tracing to hide tiles/entities that are not visible", "Oculta entidades y bloques fuera del campo de visión mediante cálculo asíncrono para subir los FPS."),
        ("adds dungeons of various rarities and sizes", "Añade mazmorras épicas de distintos tamaños y dificultades en el mundo."),
        ("camera mod with focus on process and aesthetics", "Mod de cámara fotográfica con revelado de fotos y enfoque estético inmersivo."),
        ("welcome to the world of fantasy equipment! this mod adds awesome armor sets", "Añade impresionantes conjuntos de armaduras de fantasía con diseños únicos."),
        ("welcome to the world of fantasy equipment! this mod adds awesome weapons", "Añade un extenso arsenal de armas de fantasía: espadas, dagas, lanzas y mandobles."),
        ("simple efficient mod to yield higher view distances on client side", "Permite aumentar la distancia de visión en el cliente sin recargar el servidor ni perder FPS."),
        ("a library with some cool features. idk what other things to write here", "Librería base de desarrollo requerida por mods complementarios."),
        ("reduces memory usage", "Reduce drásticamente el consumo de memoria RAM en modelos de bloques y texturas."),
        ("makes rendering faster", "Acelera el renderizado de chunks y entidades en Minecraft."),
        ("formations adds utilities for structure datapacks", "Librería auxiliar con utilidades avanzadas para la generación de estructuras por datapacks."),
        ("formations overworld adds plenty of new structures", "Añade decenas de estructuras interesantes y ambientadas al mundo superior."),
        ("fusion allows resource packs to use additional texture and model types", "Permite a los paquetes de texturas usar texturas conectadas y modelos dinámicos avanzados."),
        ("a simple modern and secure way to configure and manage your mods", "Librería moderna y segura para configuración gráfica de mods."),
    ];

    for (en, es) in exact_matches {
        if lower.contains(&en.to_lowercase()) {
            return es.to_string();
        }
    }

    let mut result = trimmed.to_string();

    let replacements = [
        ("A modern and fast", "Un moderno y rápido"),
        ("A modern", "Un moderno"),
        ("A performance mod", "Un mod de rendimiento"),
        ("A client-side mod that", "Un mod exclusivo de cliente que"),
        ("A server-side mod that", "Un mod de servidor que"),
        ("A mod that optimizes", "Un mod que optimiza"),
        ("A mod that adds", "Un mod que añade"),
        ("A mod that allows players to", "Un mod que permite a los jugadores"),
        ("A mod that allows you to", "Un mod que te permite"),
        ("A mod that allows", "Un mod que permite"),
        ("A mod that fixes", "Un mod que corrige"),
        ("A mod that improves", "Un mod que mejora"),
        ("A mod designed to", "Un mod diseñado para"),
        ("A simple mod that", "Un mod sencillo que"),
        ("A lightweight mod that", "Un mod ligero que"),
        ("A mod that", "Un mod que"),
        ("A library for", "Una librería para"),
        ("A library mod", "Un mod de librería"),
        ("An optimization mod", "Un mod de optimización"),
        ("allowing players to", "permitiendo a los jugadores"),
        ("allows players to", "permite a los jugadores"),
        ("allows you to", "te permite"),
        ("lets you", "te permite"),
        ("aims to improve", "tiene como objetivo mejorar"),
        ("aims to optimize", "busca optimizar"),
        ("Optimizes chunk loading and saving", "Optimiza la carga y guardado de chunks"),
        ("Optimizes chunk loading and generation", "Optimiza la carga y generación de chunks"),
        ("Optimizes chunk loading", "Optimiza la carga de chunks"),
        ("Optimizes chunk generation", "Optimiza la generación de chunks"),
        ("Optimizes the game", "Optimiza el juego"),
        ("Reduces memory usage", "Reduce el uso de memoria RAM"),
        ("Reduces RAM usage", "Reduce el consumo de RAM"),
        ("Improves performance and FPS", "Mejora el rendimiento y los FPS"),
        ("Improves performance", "Mejora el rendimiento general"),
        ("Improves FPS", "Aumenta la tasa de cuadros por segundo (FPS)"),
        ("Quality of life mod", "Mod de comodidad y calidad de vida (QoL)"),
        ("Quality of life", "Calidad de vida (QoL)"),
        ("Client-side only", "Solo del lado del cliente"),
        ("Server-side only", "Solo del lado del servidor"),
        ("Adds support for", "Añade compatibilidad para"),
        ("Adds new biomes, structures", "Añade nuevos biomas, estructuras"),
        ("Adds new", "Añade nuevos"),
        ("Adds various", "Añade varios"),
        ("Adds helpful", "Añade útiles"),
        ("Adds over", "Añade más de"),
        ("Adds", "Añade"),
        ("Fixes bugs and crashes", "Corrige errores y cierres inesperados"),
        ("Fixes bugs and", "Corrige errores y"),
        ("Fixes memory leaks", "Corrige fugas de memoria"),
        ("Fixes", "Corrige"),
        ("Faster world loading", "Carga del mundo más rápida"),
        ("Better performance", "Mejor rendimiento"),
        ("Configurable in", "Configurable en"),
        ("Required by", "Requerido por"),
        ("Dependency for", "Dependencia para"),
        ("Provides an API for", "Proporciona una API para"),
        ("A collection of", "Una colección de"),
        ("Allows you to", "Te permite"),
        ("Enables you to", "Te permite"),
        ("Enhances the", "Mejora el"),
        ("Overhauls the", "Rediseña por completo el"),
        ("Replaces the", "Reemplaza el"),
        ("designed to improve", "diseñado para mejorar"),
        ("designed to optimize", "diseñado para optimizar"),
        ("designed to fix", "diseñado para corregir"),
        ("which makes Minecraft run much faster", "que hace que Minecraft funcione mucho más rápido"),
        ("Purely aesthetic", "Pura estética"),
        ("for Minecraft", "para Minecraft"),
    ];

    for (en, es) in replacements {
        let pattern = format!("(?i){}", regex::escape(en));
        if let Ok(re) = crate::scanner::cached_regex(&pattern) {
            result = re.replace_all(&result, es).to_string();
        }
    }

    result
}

pub fn get_mod_detailed_info(
    mod_id: &str,
    clean_slug: &str,
    display_name: &str,
    raw_desc: &str,
    category: &str,
) -> DetailedModPayload {
    let slug_lower = clean_slug.to_lowercase();
    let id_lower = mod_id.to_lowercase();
    let _name_lower = display_name.to_lowercase();

    // 1. Curated databases for key Minecraft mods
    if slug_lower.contains("sodium") || slug_lower.contains("embeddium") || slug_lower.contains("rubidium") || id_lower.contains("sodium") || id_lower.contains("embeddium") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Rendimiento".to_string(),
            description_es: "Motor de renderizado moderno de última generación para Minecraft. Reemplaza por completo el sistema gráfico vanilla de OpenGL obsoleto, multiplicando la tasa de cuadros por segundo (FPS) y reduciendo el tartamudeo (stuttering).".to_string(),
            features: vec![
                "Reescritura total del pipeline de renderizado de chunks con sombreado de vértices moderno.".to_string(),
                "Reducción drástica del uso de CPU y llamadas de dibujo (draw calls).".to_string(),
                "Compatibilidad nativa con sombreadores gráficos (shaders) mediante Iris o Oculus.".to_string(),
                "Panel de opciones gráficas avanzado con ajustes personalizables de rendimiento.".to_string(),
            ],
            commands: vec![
                "/sodium (abre el menú de configuración si está disponible)".to_string(),
                "Ajustes accesibles directamente en Opciones de Vídeo de Minecraft.".to_string(),
            ],
            wiki_url: "https://github.com/CaffeineMC/sodium-fabric/wiki".to_string(),
            source_url: "https://github.com/CaffeineMC/sodium-fabric".to_string(),
            config_tip: "Menú de Opciones de Vídeo -> sodium-options.json".to_string(),
        };
    }

    if slug_lower.contains("c2me") || id_lower.contains("c2me") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Rendimiento".to_string(),
            description_es: "Concurrent Chunk Management Engine: mod de optimización profunda diseñado para acelerar la generación, carga, lectura y guardado de chunks aprovechando todos los núcleos lógicos del procesador mediante multihilo real.".to_string(),
            features: vec![
                "Generación de terreno asíncrona y multihilo en segundo plano.".to_string(),
                "Entrada/Salida (I/O) optimizada sin bloqueos en el hilo principal del juego.".to_string(),
                "Vuelo ultra-fluido en elytras o teletransportes rápidos sin que el mundo desaparezca bajo tus pies.".to_string(),
                "Optimización de memoria en caché de chunks no visitados.".to_string(),
            ],
            commands: vec![
                "/c2me status (muestra estadísticas de hilos activos y cola de chunks)".to_string(),
                "/c2me debug (información detallada para depuración)".to_string(),
            ],
            wiki_url: "https://github.com/RelativityMC/C2ME-fabric/wiki".to_string(),
            source_url: "https://github.com/RelativityMC/C2ME-fabric".to_string(),
            config_tip: "config/c2me.toml - Permite ajustar el número de hilos de CPU asignados a la generación.".to_string(),
        };
    }

    if slug_lower.contains("ferritecore") || id_lower.contains("ferritecore") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Rendimiento".to_string(),
            description_es: "Optimización extrema del consumo de memoria RAM en Minecraft. Deduplica y comprime los modelos de bloques de estado en memoria sin alterar el aspecto visual.".to_string(),
            features: vec![
                "Ahorro de hasta 1 a 2 GB de memoria RAM en modpacks medianos y pesados.".to_string(),
                "Deduplicación de modelos y estados de bloque cacheados.".to_string(),
                "Reduce los picos de recolección de basura (GC pauses), disminuyendo los congelamientos repentinos.".to_string(),
            ],
            commands: vec![
                "/ferritecore dump (vuelca estadísticas de memoria y objetos deduplicados)".to_string(),
            ],
            wiki_url: "https://github.com/malte-sw/FerriteCore".to_string(),
            source_url: "https://github.com/malte-sw/FerriteCore".to_string(),
            config_tip: "config/ferritecore.mixin.properties".to_string(),
        };
    }

    if slug_lower.contains("clearlag") || id_lower.contains("clearlag") || _name_lower.contains("clearlag") || _name_lower.contains("clear lag") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Rendimiento".to_string(),
            description_es: "Mod de optimización y limpieza de entidades para mundos y servidores. Elimina periódica o manualmente los objetos tirados en el suelo (ground items) y restos acumulados en estructuras masivas, liberando memoria y evitando caídas de FPS.".to_string(),
            features: vec![
                "Limpieza automática programada de objetos en el suelo a intervalos configurables.".to_string(),
                "Avisos y notificaciones en el chat con cuenta regresiva antes de cada purga.".to_string(),
                "Comando de ejecución manual inmediata para liberar entidades acumuladas al instante.".to_string(),
                "Temporizador ajustable y personalizable para mundos con granjas técnicas o servidores.".to_string(),
            ],
            commands: vec![
                "/clearlagnow (o /clearlagn): Ejecuta una limpieza manual inmediata de objetos del suelo.".to_string(),
                "/clearlagset <segundos>: Cambia el intervalo de tiempo entre limpiezas automáticas (por defecto 300s / 5 min).".to_string(),
                "/clearlagnotify <true/false>: Activa o desactiva las notificaciones de aviso en el chat.".to_string(),
            ],
            wiki_url: "https://modrinth.com/mod/clearlag".to_string(),
            source_url: "https://www.curseforge.com/minecraft/mc-mods/clearlag".to_string(),
            config_tip: "Requiere TCT Core como dependencia. Ajustes mediante comandos en el juego o en config/clearlag.json.".to_string(),
        };
    }

    if slug_lower.contains("modernfix") || id_lower.contains("modernfix") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Rendimiento".to_string(),
            description_es: "Conjunto maestro de correcciones de rendimiento, arranque acelerado y solución de fugas de memoria para Minecraft Forge, NeoForge y Fabric.".to_string(),
            features: vec![
                "Reduce el tiempo de arranque del juego hasta un 50% al evitar cálculos redundantes.".to_string(),
                "Soluciona fugas de memoria críticas generadas por estructuras y entidades.".to_string(),
                "Optimizaciones profundas en el gestor de recursos y recetas dinámicas.".to_string(),
                "Acelera la pantalla de carga al unirse a mundos o servidores.".to_string(),
            ],
            commands: vec![
                "Mod pasivo de bajo nivel: actúa durante el arranque y ejecución del juego.".to_string(),
            ],
            wiki_url: "https://github.com/embeddedt/ModernFix/wiki".to_string(),
            source_url: "https://github.com/embeddedt/ModernFix".to_string(),
            config_tip: "config/modernfix/modernfix.properties".to_string(),
        };
    }

    if slug_lower.contains("spark") || id_lower.contains("spark") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Rendimiento".to_string(),
            description_es: "Potente herramienta de perfilado de rendimiento (profiler) para Minecraft. Diagnostica con precisión milimétrica qué mod, entidad o proceso provoca caídas de TPS o FPS.".to_string(),
            features: vec![
                "Muestreo de CPU en tiempo real con árbol de llamadas interactivo.".to_string(),
                "Monitoreo de TPS, uso de memoria, frecuencia de GC y latencia de red.".to_string(),
                "Genera reportes web interactivos detallados con un solo comando.".to_string(),
                "Inspección de entidades activas y chunks cargados causantes de retrasos.".to_string(),
            ],
            commands: vec![
                "/spark sampler --start --timeout 30 (inicia perfilado de 30 segundos)".to_string(),
                "/spark healthreport (muestra reporte instantáneo de TPS, CPU y RAM)".to_string(),
                "/spark tps (monitorea TPS en tiempo real)".to_string(),
                "/spark gc (ejecuta o monitorea la recolección de basura)".to_string(),
            ],
            wiki_url: "https://spark.lucko.me/docs".to_string(),
            source_url: "https://github.com/lucko/spark".to_string(),
            config_tip: "spark genera enlaces web seguros para compartir diagnósticos en foros o discord.".to_string(),
        };
    }

    if slug_lower.contains("distanthorizons") || slug_lower.contains("distant_horizons") || id_lower.contains("distanthorizons") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Gráficos, Cámara y Animaciones".to_string(),
            description_es: "Añade distancias de renderizado simplificadas mediante LOD (Nivel de Detalle), permitiendo contemplar paisajes a más de 128 a 512 chunks de distancia con mínimo impacto de rendimiento.".to_string(),
            features: vec![
                "Renderizado de terreno lejano a escalas épicas sin colapsar la memoria ni los FPS.".to_string(),
                "Mapeo de elevación e iluminación dinámica a gran escala.".to_string(),
                "Integración con shaders modernos (Iris / Oculus en versiones compatibles).".to_string(),
            ],
            commands: vec![
                "/dh (abre el menú de configuración gráfica en pantalla)".to_string(),
                "/dh show (muestra información de diagnóstico de la caché LOD)".to_string(),
            ],
            wiki_url: "https://gitlab.com/distant-horizons/distant-horizons/-/wikis/home".to_string(),
            source_url: "https://gitlab.com/distant-horizons/distant-horizons".to_string(),
            config_tip: "config/DistantHorizons.toml".to_string(),
        };
    }

    if slug_lower.contains("immediatelyfast") || id_lower.contains("immediatelyfast") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Rendimiento".to_string(),
            description_es: "Optimiza de forma masiva el renderizado en modo inmediato de Minecraft: texto de menús, barras de jefes, HUD, etiquetas de nombres de mobs, interfaces de cofres y mapas.".to_string(),
            features: vec![
                "Elimina el retraso y lag gráfico al abrir inventarios o menús con mucho texto.".to_string(),
                "Acelera el renderizado de partículas 2D y barras de progreso en pantalla.".to_string(),
                "Multiplica la fluidez en servidores con muchos jugadores o hologramas.".to_string(),
            ],
            commands: vec![
                "Mod pasivo automático: no requiere comandos en el chat.".to_string(),
            ],
            wiki_url: "https://github.com/Raphiiko/ImmediatelyFast/wiki".to_string(),
            source_url: "https://github.com/Raphiiko/ImmediatelyFast".to_string(),
            config_tip: "config/immediatelyfast.json".to_string(),
        };
    }

    if slug_lower.contains("entityculling") || id_lower.contains("entityculling") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Rendimiento".to_string(),
            description_es: "Utiliza algoritmos de oclusión ambiental para omitir el renderizado de entidades (mobs, cofres, marcos, soportes de armadura) que se encuentren ocultas detrás de paredes o bajo tierra.".to_string(),
            features: vec![
                "Evita que el juego dibuje cientos de mobs o cofres que están fuera de tu visión directa.".to_string(),
                "Aumento gigantesco de FPS cerca de almacenes grandes, aldeas o granjas de mobs.".to_string(),
                "Uso de trazado de rayos en subhilos sin penalización de CPU en el juego principal.".to_string(),
            ],
            commands: vec![
                "Mod pasivo de optimización visual.".to_string(),
            ],
            wiki_url: "https://github.com/tr7zw/EntityCulling".to_string(),
            source_url: "https://github.com/tr7zw/EntityCulling".to_string(),
            config_tip: "config/entityculling.json".to_string(),
        };
    }

    if slug_lower.contains("chunky") || id_lower.contains("chunky") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Rendimiento".to_string(),
            description_es: "Herramienta esencial para precargar y pregenerar el mundo y sus dimensiones en segundo plano, erradicando el lag de generación al explorar por primera vez.".to_string(),
            features: vec![
                "Genera chunks por adelantado de forma rápida y multihilo.".to_string(),
                "Soporte para áreas circulares, cuadradas o poligonales personalizables.".to_string(),
                "Pausa y reanudación automática segura sin corromper el mapa.".to_string(),
            ],
            commands: vec![
                "/chunky radius <bloques> (define el radio a pregenerar)".to_string(),
                "/chunky start (inicia el proceso de pregeneración)".to_string(),
                "/chunky pause (pausa la pregeneración temporalmente)".to_string(),
                "/chunky progress (muestra el porcentaje completado y chunks/s)".to_string(),
            ],
            wiki_url: "https://github.com/pop4959/Chunky/wiki".to_string(),
            source_url: "https://github.com/pop4959/Chunky".to_string(),
            config_tip: "Ejecutar preferiblemente en el servidor o mundo antes de empezar a jugar.".to_string(),
        };
    }

    if slug_lower.contains("jei") || slug_lower.contains("rei") || slug_lower.contains("emi") || id_lower.contains("jei") || id_lower.contains("roughlyenoughitems") || id_lower.contains("emi") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Calidad de Vida".to_string(),
            description_es: "Explorador universal de recetas, usos de objetos, componentes de crafteo y alquimia integrado en la pantalla del inventario.".to_string(),
            features: vec![
                "Visualización completa de recetas al pulsar 'R' o hacer clic sobre cualquier objeto.".to_string(),
                "Visualización de los usos de un objeto pulsando 'U'.".to_string(),
                "Búsqueda por nombre, mod (@mod_name) o etiquetas (#tag).".to_string(),
                "Marcador de objetos favoritos y cálculo de componentes necesarios para fabricación.".to_string(),
            ],
            commands: vec![
                "Tecla 'R': Ver receta de crafteo.".to_string(),
                "Tecla 'U': Ver usos del ítem.".to_string(),
                "Ctrl + F: Enfocar la barra de búsqueda de recetas.".to_string(),
            ],
            wiki_url: "https://github.com/mezz/JustEnoughItems/wiki".to_string(),
            source_url: "https://github.com/mezz/JustEnoughItems".to_string(),
            config_tip: "Configurable desde el icono de llave inglesa dentro del inventario.".to_string(),
        };
    }

    if slug_lower.contains("jade") || slug_lower.contains("wthit") || id_lower.contains("jade") || id_lower.contains("wthit") {
        return DetailedModPayload {
            mod_id: mod_id.to_string(),
            name: display_name.to_string(),
            category: "Interfaz".to_string(),
            description_es: "Indicador HUD informativo en pantalla. Muestra exactamente qué bloque, entidad o cultivo estás mirando, incluyendo mod al que pertenece, vida restante, contenido de cofres y progreso de crecimiento.".to_string(),
            features: vec![
                "Muestra nombre del bloque y mod de origen al apuntar la mira.".to_string(),
                "Previsualización de los ítems dentro de cofres, barriles o tolvas sin abrirlos.".to_string(),
                "Porcentaje de crecimiento de cultivos y tiempo restante.".to_string(),
                "Vida actual y efectos de estado de mobs pasivos y hostiles.".to_string(),
            ],
            commands: vec![
                "Tecla '0' del teclado numérico: Abre el menú de configuración de Jade.".to_string(),
            ],
            wiki_url: "https://github.com/Snownee/Jade/wiki".to_string(),
            source_url: "https://github.com/Snownee/Jade".to_string(),
            config_tip: "Menú de opciones accesible con tecla '0' (configurable en controles).".to_string(),
        };
    }

    // 2. Generic fallback with intelligent translation & smart feature extraction
    let translated_desc = if !raw_desc.is_empty() {
        translate_to_spanish(raw_desc)
    } else {
        format!("Mod especializado en la categoría de {}.", category.to_lowercase())
    };

    let mut generated_features = Vec::new();
    if category == "Rendimiento" {
        generated_features.push("Optimiza el flujo de ejecución para reducir el retraso (lag) y elevar los FPS.".to_string());
        generated_features.push("Mejora la eficiencia en el uso de memoria o hilos del procesador.".to_string());
    } else if category == "Graficos" {
        generated_features.push("Mejora la fidelidad visual, iluminación o renderizado de texturas y modelos.".to_string());
    } else if category == "Mundo" {
        generated_features.push("Añade o modifica generación de biomas, terreno o dimensiones en el mundo.".to_string());
    } else if category == "Estructuras" {
        generated_features.push("Añade nuevas mazmorras, aldeas, fortalezas o ruinas explorables en el mundo.".to_string());
    } else if category == "Calidad de vida" || category == "QoL" {
        generated_features.push("Añade comodidades de juego, atajos y mejoras de usabilidad en inventarios o mecánicas.".to_string());
    } else if category == "UI" {
        generated_features.push("Proporciona información visual enriquecida en la pantalla o interfaz de usuario.".to_string());
    } else if category == "Dependencias" || category == "API" {
        generated_features.push("Librería de soporte técnico y dependencias requerida por otros mods para funcionar.".to_string());
    } else {
        generated_features.push(format!("Añade contenido y mecánicas de {}", category.to_lowercase()));
    }

    let search_slug = if !clean_slug.is_empty() { clean_slug } else { mod_id };
    let wiki_url = format!("https://modrinth.com/mod/{}", search_slug);
    let source_url = format!("https://github.com/search?q={}", search_slug);

    DetailedModPayload {
        mod_id: mod_id.to_string(),
        name: display_name.to_string(),
        category: category.to_string(),
        description_es: translated_desc,
        features: generated_features,
        commands: vec!["Mod pasivo: funciona automáticamente sin comandos en el chat.".to_string()],
        wiki_url,
        source_url,
        config_tip: format!("Revisa la carpeta config/ para personalizar las opciones de este mod."),
    }
}

