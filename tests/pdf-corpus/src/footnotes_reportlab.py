# Builds ../footnotes.pdf (reportlab) and footnotes.md, the text it should
# come back as: two pages with a running header, page numbers, superscript
# note markers whose notes sit at the bottom of their page under a short
# rule, coloured text, and a ruled table.
from reportlab.lib import colors
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import getSampleStyleSheet
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, PageBreak, Table, TableStyle
styles = getSampleStyleSheet()
notes = {
    1: [(1, "The markers were set in concrete in the autumn before the survey."), (2, "Readings after dark were taken by lamp and checked twice.")],
    2: [(3, "Counts from the evening walk were not used.")],
}
def page_furniture(c, doc):
    c.saveState()
    c.setFont("Helvetica", 8)
    c.drawString(72, A4[1] - 40, "Survey of the lower valley")
    c.drawCentredString(A4[0] / 2, 30, f"{doc.page}")
    page_notes = notes.get(doc.page, [])
    y = 60 + 12 * (len(page_notes) - 1)
    c.setLineWidth(0.5)
    c.line(72, y + 12, 200, y + 12)
    for n, t in page_notes:
        c.setFont("Helvetica", 6)
        c.drawString(72, y + 3, str(n))
        c.setFont("Helvetica", 8)
        c.drawString(78, y, t)
        y -= 12
    c.restoreState()
body = styles["BodyText"]
story, md = [], []
def h(t):
    story.append(Paragraph(t, styles["Heading1"]))
    md.append(f"# {t}\n")
def p(html, text):
    story.append(Paragraph(html, body))
    story.append(Spacer(1, 6))
    md.append(text + "\n")
h("Survey of the lower valley")
p("The survey team placed seven markers along the lower river<super>1</super> and read them every morning and evening for six weeks.",
  "The survey team placed seven markers along the lower river[^1] and read them every morning and evening for six weeks.")
p('Evening readings were harder<super>2</super> because the light failed early in March. <font color="#1F5C99">Blue figures</font> in the field books mark readings that were repeated.',
  "Evening readings were harder[^2] because the light failed early in March. Blue figures in the field books mark readings that were repeated.")
h("Readings")
rows = [["Marker", "Morning", "Evening"], ["Mill", "2.1", "2.4"], ["Ferry", "0.9", "1.1"], ["Bridge", "1.6", "1.5"]]
t = Table(rows, colWidths=[90, 80, 80])
t.setStyle(TableStyle([("GRID", (0, 0), (-1, -1), 0.5, colors.grey), ("FONTNAME", (0, 0), (-1, 0), "Helvetica-Bold")]))
story.append(t)
md.append("| Marker | Morning | Evening |\n|---|---|---|\n| Mill | 2.1 | 2.4 |\n| Ferry | 0.9 | 1.1 |\n| Bridge | 1.6 | 1.5 |\n")
story.append(PageBreak())
h("Birds")
p("The heron count was highest in the second week<super>3</super>, when eleven birds stood in the reeds below the mill.",
  "The heron count was highest in the second week[^3], when eleven birds stood in the reeds below the mill.")
p("The team will repeat the survey next spring with two more markers.", "The team will repeat the survey next spring with two more markers.")
for k, page_notes in notes.items():
    for n, text in page_notes:
        md.append(f"[^{n}]: {text}\n")
SimpleDocTemplate("../footnotes.pdf", pagesize=A4, title="Survey").build(story, onFirstPage=page_furniture, onLaterPages=page_furniture)
open("footnotes.md", "w").write("\n".join(md))
