# Changelog / Cambios

Each release lists its changes in English and in Spanish. The release workflow
copies the section of the version being published into the GitHub release.

Cada versión lista sus cambios en inglés y en español. El flujo de publicación
copia la sección de la versión que se publica a la release de GitHub.

## [0.1.35]

### English

- Windows' own volume and brightness indicator no longer pops up over the
  shell: while Norbanner's sliders are on, they show the level instead,
  brightness keys included.
- The lyrics and the song's progress no longer jump when pausing or
  resuming Spotify (and other players that report their position only on
  a seek, play or pause).
- Lyrics prefer the version timed on the very recording that plays: one as
  long as the song wins over versions a few seconds off, which may have a
  different intro.
- Dark themes outline the frame and the drawers with a thin line, so they
  stand out from dark windows and wallpapers.
- With several screens, each one's brightness is read again correctly.

### Español

- El indicador de volumen y brillo de Windows ya no aparece sobre el shell:
  mientras los controles de Norbanner están activos, ellos muestran el
  nivel, también con las teclas de brillo.
- La letra y el progreso de la canción ya no saltan al pausar o reanudar
  Spotify (y otros reproductores que solo informan su posición al saltar,
  reproducir o pausar).
- La letra prefiere la versión sincronizada con la grabación que suena: una
  que dura lo mismo que la canción gana a versiones unos segundos más
  largas o cortas, que pueden tener otra introducción.
- Los temas oscuros dibujan una línea fina en el borde del marco y los
  cajones, para que se distingan de ventanas y fondos oscuros.
- Con varias pantallas, el brillo de cada una se vuelve a leer bien.

## [0.1.34]

### English

- Popular songs get their lyrics: the lyrics service answers a search with
  every version of the song, often over 512 KB, and answers that large were
  rejected as failures. They are accepted now.
- The word sweep keeps pace with slow lines: a line is spread over most of
  the time until the next one instead of an assumed reading speed, which
  ran ahead of the voice.

### Español

- Las canciones populares ya tienen letra: el servicio de letras responde a
  una búsqueda con todas las versiones de la canción, a menudo más de
  512 KB, y las respuestas tan grandes se descartaban como fallos. Ahora se
  aceptan.
- El barrido de palabras sigue el ritmo de los versos lentos: el verso se
  reparte en casi todo el tiempo hasta el siguiente en lugar de una
  velocidad de lectura supuesta, que se adelantaba a la voz.

## [0.1.33]

### English

- Songs no longer stay without lyrics after a network hiccup: a lookup that
  could not reach the service is tried again every few seconds while the
  song plays, instead of counting as "no lyrics" for the whole session.
- The volume and brightness sliders on the right now include brightness
  for a laptop's built-in screen (external monitors keep using DDC/CI).
- The tray button in the bar opens the background apps bubble instead of
  Windows' tray, and the bubble's arrow to Windows' tray is gone.

### Español

- Las canciones ya no se quedan sin letra después de un fallo de red: una
  búsqueda que no pudo llegar al servicio se reintenta cada pocos segundos
  mientras suena la canción, en lugar de contar como "sin letra" toda la
  sesión.
- Los deslizadores de volumen y brillo de la derecha incluyen ahora el
  brillo de la pantalla integrada de los portátiles (los monitores externos
  siguen usando DDC/CI).
- El botón de la bandeja de la barra abre la burbuja de apps en segundo
  plano en lugar de la bandeja de Windows, y se quitó la flecha de la
  burbuja que llevaba a la bandeja de Windows.

## [0.1.32]

### English

- The lyrics island finds lyrics for more songs. Music video titles such as
  "Song (Official Video)" or "Artist - Song", and guest lists in the
  artist, are cleaned before the search, and a general search follows when
  the first finds nothing.
- Songs whose length differs from every version the service has (a video
  with a longer intro, a radio edit) now use the closest version up to
  20 seconds away instead of showing no lyrics; scrolling over the island
  corrects the timing if needed.

### Español

- La isla de letras encuentra la letra de más canciones. Los títulos de
  videoclips como «Canción (Official Video)» o «Artista - Canción», y las
  listas de invitados en el artista, se limpian antes de buscar, y si la
  primera búsqueda no encuentra nada se hace una búsqueda general.
- Las canciones cuya duración no coincide con ninguna versión del servicio
  (un videoclip con una intro más larga, una versión de radio) usan ahora
  la versión más cercana hasta 20 segundos de diferencia en lugar de no
  mostrar letra; girar la rueda sobre la isla corrige el tiempo si hace
  falta.

## [0.1.31]

### English

- The lyrics sweep no longer runs ahead of the voice: it moves through each
  word at the song's own position, from its start to the next word's,
  instead of filling it as soon as it starts. Lines appear 80 ms early
  instead of 200 ms.
- Apps in the tray bubble can be closed: hover one and press its ✕. The
  app is asked to close first; if it is still running three seconds later
  (tray apps often only hide), it is ended.

### Español

- El barrido de la letra ya no se adelanta a la voz: avanza dentro de cada
  palabra según la posición real de la canción, desde que empieza hasta que
  empieza la siguiente, en lugar de rellenarla en cuanto empieza. Los versos
  aparecen 80 ms antes en lugar de 200 ms.
- Las apps de la burbuja de la bandeja se pueden cerrar: pasa el puntero por
  una y pulsa su ✕. Primero se le pide que se cierre; si a los tres segundos
  sigue en marcha (las apps de la bandeja a menudo solo se esconden), se
  finaliza.

## [0.1.30]

### English

- The dashboard's size can be set in Settings → Panels (70–150 %).
- Lyrics stay in time after a song change: the lookup waits until the
  player reports the song's length, so another recording's timings are no
  longer taken and kept.
- Four lyrics animations, in Settings → Panels: sweep (word by word fill),
  word (the word being sung lights up), line (whole lines slide) and fade.
  The lyrics island can also be switched on or off there.
- Hovering the tray button opens a bubble with the apps running in the
  background. A click brings the app forward or opens it again; the last
  button opens Windows' tray, where each icon's own menu is.

### Español

- El tamaño del panel superior se ajusta en Configuración → Paneles
  (70–150 %).
- La letra sigue a tiempo después de cambiar de canción: la búsqueda espera
  a que el reproductor informe de la duración, así que ya no se toman ni se
  guardan los tiempos de otra grabación.
- Cuatro animaciones para la letra, en Configuración → Paneles: barrido
  (relleno palabra por palabra), palabra (se ilumina la que se canta), verso
  (versos enteros que suben) y fundido. Allí también se activa o desactiva
  la isla de letras.
- Al pasar el puntero por el botón de la bandeja se abre una burbuja con las
  apps en segundo plano. Un clic trae la app al frente o la vuelve a abrir;
  el último botón abre la bandeja de Windows, donde está el menú de cada
  icono.

## [0.1.29]

### English

- The windows panel shows a picture of each window beside its title,
  taken when the panel opens (minimized windows show the app's icon).
- Closing a window from the panel takes it off the list, and the panel
  closes with the app's last window instead of staying until the pointer
  leaves.
- The lyrics island shows only on the bare desktop: it tucks away while an
  app is in front and comes back with the desktop.
- The island shows the song's title and artist, small, beside a wave that
  moves while the song plays.

### Español

- El panel de ventanas muestra una imagen de cada ventana junto a su
  título, tomada al abrirse el panel (las ventanas minimizadas muestran el
  icono de la app).
- Cerrar una ventana desde el panel la quita de la lista, y el panel se
  cierra con la última ventana de la app en lugar de quedarse hasta que el
  puntero sale.
- La isla de letras solo aparece en el escritorio: se recoge mientras hay
  una app al frente y vuelve con el escritorio.
- La isla muestra el título y el artista de la canción, en pequeño, junto a
  una onda que se mueve mientras suena.

## [0.1.28]

### English

- The lyrics island keeps time with the song. Lyric services hold several
  volunteer timings of the same recording that can disagree by seconds;
  Norbanner now takes the one most of them agree on, using the song's
  album to break ties.
- A sweep of colour fills the line word by word as it is sung.
- Scrolling over the island moves the lyrics earlier or later by a quarter
  second, remembered for that song until the shell restarts.

### Español

- La isla de letras va al ritmo de la canción. Los servicios de letras
  guardan varias sincronizaciones de la misma grabación hechas por
  voluntarios que pueden diferir en segundos; Norbanner toma ahora aquella
  en la que más coinciden, y usa el álbum de la canción para desempatar.
- Un barrido de color rellena el verso palabra por palabra a medida que se
  canta.
- Girar la rueda del ratón sobre la isla adelanta o atrasa la letra un
  cuarto de segundo, y se recuerda para esa canción hasta reiniciar la
  shell.

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
