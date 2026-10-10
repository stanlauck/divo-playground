// SPDX-License-Identifier: MIT OR Apache-2.0
#set page(paper: "a4", margin: 20mm)
#set text(size: 10pt)
#set par(leading: 0.7em)
#set heading(numbering: "1.1")
#set document(title: "Field Lantern — production report", author: "Example Crew", date: datetime(year: 2026, month: 10, day: 10))
#set text(lang: "ru")

#text(size: 22pt, weight: "bold", "Field Lantern — production report")

#text(size: 13pt, "Synthetic planning review")

#text("Example Crew")

#text("2026-10-10")

#text("An invented miniature production: a paper lantern crosses a painted courtyard. Все данные вымышлены.")

#heading(level: 1, text("Overview"))

#text("Plan #1: keep *all* checks literal. No <markup>, [links], or hidden #code. Русский текст: фонарь готов.")

#heading(level: 2, outlined: false, text("Daily checks"))

#list(text("Prepare paper props"),text("Check the quiet courtyard"),)

#enum(text("Place the lantern"),text("Record the invented rehearsal"),)

#table(columns: 3, inset: 7pt, stroke: 0.5pt + rgb("d4dce5"),
  table.header(strong(text("Area")),strong(text("Status")),strong(text("Count")),),
  text("Courtyard | north"),text("Ready *pending* check"),text("3"),
  text("Workshop"),text("Свет готов"),text("2"),
)

#text(size: 9pt, "Synthetic resource table")

#table(columns: 2, inset: 7pt, stroke: 0.5pt + rgb("d4dce5"),
  table.header(strong(text("Key")),strong(text("Value")),),
  text("Coordinator"),text("Example Crew"),
  text("Stage"),text("Paper courtyard"),
)

#raw("cue = \"lantern\"\n# literal * _ @ < > $ ` ~ // \\ \"\n```", block: true, lang: "text")

#quote(block: true, text("Small props can tell a large invented story."), attribution: text("Fictional planning note"))

#block(width: 100%, inset: 10pt, radius: 4pt, fill: rgb("e9f1fa"))[#strong(text("Note:")) #text("All counts are synthetic.")]

#block(width: 100%, inset: 10pt, radius: 4pt, fill: rgb("fff3d6"))[#strong(text("Warning:")) #text("Confirm the paper rig before rehearsal.")]

#block(width: 100%, inset: 10pt, radius: 4pt, fill: rgb("fde8e8"))[#strong(text("Error:")) #text("Example issue: spare lantern not yet assigned.")]

#block(width: 100%, inset: 10pt, radius: 4pt, fill: rgb("e6f4eb"))[#strong(text("Success:")) #text("Painted backdrop check complete.")]

#heading(level: 2, text("Resource detail"))

#text("Two paper lanterns and three painted panels are reserved.")

#heading(level: 1, text("Visual plan"))

#pagebreak()

#block(width: 65%, height: 48pt, inset: 8pt, stroke: 0.5pt + gray)[#text("Image unavailable: missing-lantern.png")]

#text(size: 9pt, "Invented lantern reference (placeholder)")

