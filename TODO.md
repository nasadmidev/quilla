# Quilla TODO

Quilla está en la versión preliminar `0.1.0`. Esta lista recoge el trabajo previsto para completar sus funciones principales y preparar una primera versión estable.

## 1. Flujo de instalación y experiencia de uso

- [ ] Añadir concurrencia o un flujo asíncrono para descargar varios paquetes a la vez.
- [ ] Mostrar barras de progreso durante la descarga y la extracción de paquetes.
- [ ] Mostrar qué paquete se está procesando y cuándo termina cada etapa de la instalación.
- [ ] Mejorar los mensajes de error: contexto útil, formato consistente y colores opcionales.
- [ ] Añadir `--help` y `--version`.
- [ ] Añadir un modo de simulación (`--dry-run`) que muestre los cambios previstos sin aplicarlos.

## 2. Operaciones de paquetes y generaciones

- [ ] Implementar `rollback` para activar una generación anterior y sincronizar el estado de la base de datos.
- [ ] Implementar `remove`, incluyendo la resolución de dependencias que queden sin uso y el tratamiento de conflictos.
- [ ] Implementar `update` para actualizar los índices y los paquetes instalados según las políticas de versiones definidas.
- [ ] Definir y manejar de forma consistente paquetes reemplazados, dependencias opcionales y requisitos de versión incompatibles.
- [ ] Conservar las generaciones anteriores necesarias para permitir rollback y definir cómo se podrán eliminar.

## 3. Seguridad e integridad

- [ ] Verificar las firmas criptográficas de los índices de repositorio, además del SHA-256 de cada paquete.
- [ ] Definir el modelo de confianza de claves: instalación inicial, almacenamiento, rotación y revocación.
- [ ] Validar las rutas y los tipos de archivo de los paquetes antes y durante la extracción.
- [ ] Evitar que rutas o enlaces simbólicos de un archivo puedan escribir fuera del directorio temporal del paquete.
- [ ] Revisar el tratamiento de permisos especiales y enlaces simbólicos al guardar paquetes en el store.
- [ ] Documentar si se ejecutarán scripts de instalación de paquetes y bajo qué condiciones.

## 4. Transacciones y recuperación (Working)

- [ ] Hacer que los cambios en el perfil y la base de datos se confirmen de forma coherente.
- [ ] Recuperar o limpiar transacciones interrumpidas sin dejar el perfil y la base de datos en estados distintos.
- [ ] Definir el comportamiento ante fallos en cada etapa: descarga, verificación, extracción, escritura al store, creación de generación y activación.
- [ ] Asegurar que una generación incompleta nunca se active.

## 5. Caché y almacenamiento

- [ ] Separar claramente los archivos descargados de los directorios temporales de extracción.
- [ ] Limpiar los archivos temporales después de una instalación correcta.
- [ ] Añadir una política de limpieza de caché configurable, evitando borrar datos útiles para reintentos o diagnóstico.
- [ ] Definir cómo detectar y limpiar paquetes del store que ya no estén referenciados por generaciones conservadas.

## 6. Resolución y compatibilidad

- [ ] Definir la política de selección de versiones y repositorios cuando haya varios candidatos válidos.
- [ ] Mejorar los diagnósticos del resolver para explicar qué dependencia no se pudo satisfacer y por qué.
- [ ] Definir el soporte oficial de arquitecturas y formatos de repositorio.
- [ ] Validar la configuración al iniciar y mostrar errores que indiquen qué campo o valor es inválido.
- [ ] Definir una estrategia de migración para la base de datos local cuando cambie su formato.

## 7. Pruebas y calidad

- [x] Añadir pruebas unitarias para versiones, dependencias, conflictos y selección de candidatos.
- [x] Añadir una prueba de integración del flujo de instalación: descarga, verificación, extracción, store y generación.
- [x] Probar fallos controlados en verificación, extracción y guardado de la base de datos durante una transacción.
- [x] Probar interrupciones abruptas del proceso en distintos puntos de una transacción.
- [x] Usar un repositorio y un paquete de prueba locales para que la prueba de integración no dependa de mirrors públicos.
- [x] Probar instalaciones repetidas y verificar que paquetes, archivos y permisos del perfil sean consistentes.
- [x] Añadir comprobaciones automáticas de formato y compilación al flujo de desarrollo.

## 8. Documentación y preparación de una versión estable

- [ ] Documentar la instalación y configuración inicial de Quilla.
- [ ] Explicar cómo se activa el perfil y cómo se usan sus programas desde el sistema.
- [ ] Documentar las garantías y límites de rollback, caché, store y ejecución de scripts.
- [ ] Indicar claramente las arquitecturas, repositorios y formatos compatibles.
- [ ] Preparar notas de versión y una lista de cambios incompatibles antes de publicar una versión estable.
- [ ] Revisar que los comandos documentados estén implementados y que sus ejemplos correspondan al comportamiento real.
