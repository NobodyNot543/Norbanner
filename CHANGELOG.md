# Changelog / Cambios

Each release lists its changes in English and in Spanish. The release workflow
copies the section of the version being published into the GitHub release.

Cada versión lista sus cambios en inglés y en español. El flujo de publicación
copia la sección de la versión que se publica a la release de GitHub.

## [0.1.27]

### English

- Lyrics island: a small island at the top shows the line of the playing
  song as it is sung, with a short slide between lines. Turn it on or off
  with the note button in the dashboard's media card; bringing the pointer
  to the top still opens the full dashboard over it. Off by default: when
  on, the song's title, artist and length are sent to lrclib.net to find
  its synced lyrics. Songs without synced lyrics show no island.
- `norbanner shell lyrics toggle` switches it from scripts.

### Español

- Isla de letras: una isla pequeña arriba muestra el verso de la canción que
  suena a medida que se canta, con un deslizamiento corto entre versos. Se
  activa o desactiva con el botón de la nota en la tarjeta de música del
  panel; al llevar el puntero arriba se sigue abriendo el panel completo
  encima. Apagada por defecto: al activarla, el título, el artista y la
  duración de la canción se envían a lrclib.net para buscar su letra
  sincronizada. Las canciones sin letra sincronizada no muestran la isla.
- `norbanner shell lyrics toggle` la activa o desactiva desde scripts.

## [0.1.26]

### English

- Settings reads well in light mode: titles and labels are dark, the
  Norbanner logo sits on a dark tile, and the selected options stand out.
  The scrollbar and dropdowns follow the light scheme too.

### Español

- Configuración se lee bien en modo claro: los títulos y textos salen
  oscuros, el logo de Norbanner va sobre una placa oscura y las opciones
  seleccionadas se distinguen. La barra de desplazamiento y los
  desplegables también siguen el tema claro.

## [0.1.25]

### English

- On the bare desktop, with no app in front, the bar shows "Norbanner"
  instead of an empty space.

### Español

- En el escritorio, sin ninguna app al frente, la barra muestra "Norbanner"
  en lugar de un espacio vacío.

## [0.1.24]

### English

First public release of Norbanner.

- A Caelestia-style desktop shell for Windows. A frame surrounds every screen
  with a vertical bar on the left; drawers melt out of the frame: a dashboard
  at the top (clock, calendar, resources, media), a launcher at the bottom
  (apps, `>` commands, calculator, wallpapers, colour variants), the session
  menu and volume and brightness sliders on the right. Maximized windows stay
  inside the frame.
- The whole shell is coloured with Material You from the wallpaper. Changing
  the wallpaper in any way (Windows Settings, a slideshow, Spotlight, another
  app, Norbanner) recolours the shell to match, without a flash of the
  default colours.
- Virtual desktops appear as workspaces in the bar, with the apps open on
  each one; the window list beside an app has a close button on each row.
  Tapping the Windows key on its own opens the launcher.
- `shell.json` configuration with live reload, and `norbanner shell …`
  commands for scripts.
- Settings app with wallpaper and style, bar, panels, appearance and general
  pages, in the shell's own colours, in eight languages. It picks the bar
  logo with Windows' file dialog (with a preview) and sets how many apps each
  workspace shows.
- Wallpaper gallery with six wallpapers, and "From my PC" to set any image on
  the computer as the wallpaper.
- Updates come from github.com/NobodyNot543/Norbanner, signed with
  Norbanner's own key.

### Español

Primera versión pública de Norbanner.

- Una shell de escritorio estilo Caelestia para Windows. Un marco rodea cada
  pantalla con una barra vertical a la izquierda; los paneles se funden hacia
  fuera del marco: un panel arriba (reloj, calendario, recursos, multimedia),
  un lanzador abajo (apps, comandos con `>`, calculadora, fondos, variantes de
  color), el menú de sesión y los deslizadores de volumen y brillo a la
  derecha. Las ventanas maximizadas quedan dentro del marco.
- Toda la shell toma sus colores del fondo con Material You. Cambiar el fondo
  de cualquier forma (Configuración de Windows, una presentación, Spotlight,
  otra app, Norbanner) vuelve a colorear la shell para que combine, sin pasar
  por los colores por defecto.
- Los escritorios virtuales aparecen como espacios de trabajo en la barra,
  con las apps abiertas en cada uno; la lista de ventanas junto a una app
  tiene un botón para cerrar cada ventana. Tocar la tecla Windows sola abre
  el lanzador.
- Configuración `shell.json` que se aplica al momento, y comandos
  `norbanner shell …` para scripts.
- App de Configuración con páginas de fondo y estilo, barra, paneles,
  apariencia y general, con los colores de la shell, en ocho idiomas. Elige
  el logo de la barra con el diálogo de Windows (con vista previa) y decide
  cuántas apps se ven por escritorio.
- Galería con seis fondos, y "Desde mi PC" para poner como fondo cualquier
  imagen del equipo.
- Las actualizaciones llegan desde github.com/NobodyNot543/Norbanner,
  firmadas con la clave propia de Norbanner.
