# Quilla

[English](README.md) | Español

Quilla es un gestor de paquetes experimental escrito en Rust. Combina una instalación sencilla de paquetes con un store inmutable y perfiles basados en generaciones. El proyecto está en desarrollo y, por ahora, se ha probado con los repositorios de Arch Linux en `x86_64`.

> [!IMPORTANT]
> Actualmente, funciona en monohilo con `reqwest` configurado en bloqueante, así que es posible experimentar largas espera de tiempo para instalar un paquete. Proximamente se añadirá un enfoque asíncrono o concurrente.

## Cómo funciona

Al instalar un paquete, Quilla:

1. Lee los índices de repositorios configurados y resuelve las dependencias.
2. Descarga los archivos de los paquetes y comprueba su SHA-256 frente al valor del índice.
3. Descomprime los paquetes y los guarda en el store, organizados por paquete, versión y arquitectura. Los archivos ejecutables conservan el permiso de ejecución; el store no permite escritura.
4. Crea una generación que reúne los archivos de los paquetes mediante enlaces simbólicos y actualiza el enlace `current` del perfil.
5. Guarda en JSON la base de datos local con los paquetes y generaciones.

Los archivos de cada versión permanecen en el store. Una generación representa el conjunto de archivos disponible en un perfil.

## Estado actual

La instalación (`install` o `-S`) es la operación implementada para el flujo normal. Los comandos `remove` (`-R`), `upgrade` (`-Syu`) y `rollback` todavía son marcadores provisionales: no modifican paquetes, generaciones ni perfiles.

Quilla aún es una versión preliminar. La verificación actual comprueba el SHA-256 del paquete contra el índice descargado; todavía no verifica la firma criptográfica del índice. Pruébalo en un entorno de pruebas, no en un sistema que necesites mantener estable.

## Compilar

Requiere Rust y Cargo. Desde la raíz del proyecto:

```sh
cargo build --release
```

El ejecutable se genera en `target/release/quilla`.

## Configuración

Quilla carga un archivo TOML. La variable de entorno `QUILLA_CONFIG` permite indicar su ubicación. El archivo debe especificar las rutas de la caché, el store, el perfil y la base de datos, además de uno o más repositorios.

Ejemplo para los repositorios oficiales `core` y `extra` de Arch Linux:

```toml
[paths]
cache_dir = "/home/usuario/.cache/quilla"
store_dir = "/home/usuario/quilla/store"
profile_dir = "/home/usuario/quilla/profile"
database_path = "/home/usuario/quilla/database.json"

[[repositories]]
name = "core"
index_path = "core.db"
mirrors = ["https://geo.mirror.pkgbuild.com/$repo/os/$arch"]

[[repositories]]
name = "extra"
index_path = "extra.db"
mirrors = ["https://geo.mirror.pkgbuild.com/$repo/os/$arch"]
```

Quilla sustituye `$repo` por el nombre del repositorio y `$arch` por la arquitectura detectada. Puedes añadir más mirrors a la lista `mirrors` de cada repositorio.

## Uso

Indica el archivo de configuración al ejecutar Quilla:

```sh
QUILLA_CONFIG="$HOME/.config/quilla.config.toml" ./target/release/quilla install bash
```

También acepta varios paquetes en una misma operación:

```sh
QUILLA_CONFIG="$HOME/.config/quilla.config.toml" ./target/release/quilla install bash gzip
```

La forma corta de `install` es `-S`:

```sh
QUILLA_CONFIG="$HOME/.config/quilla.config.toml" ./target/release/quilla -S bash
```

El perfil activo queda en `profile_dir/current` y apunta a una generación dentro de `profile_dir/generations/`. La base de datos local se guarda en `database_path`.

## Estructura del proyecto

```text
src/
├── main.rs          # Entrada del programa y coordinación de comandos
├── lib.rs           # Módulos y utilidades comunes
├── cli.rs           # Lectura de comandos
├── config.rs        # Carga de la configuración TOML
├── error.rs         # Tipo de error común
├── package.rs       # Metadatos, versiones y dependencias
├── repo.rs          # Lectura de índices de repositorios Pacman
├── db.rs            # Base de datos local de paquetes y generaciones
├── resolver.rs      # Resolución de dependencias y conflictos
├── fetch.rs         # Descarga y caché
├── verify.rs        # Comprobación SHA-256 de paquetes
├── store.rs         # Store de paquetes por hash
├── generations.rs   # Creación y activación de perfiles y generaciones
└── transaction.rs   # Flujo de instalación
```

