# Builds ../fieldnotes.pdf (reportlab, a different producer from Chrome) and
# fieldnotes.md, the text it should come back as.
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import getSampleStyleSheet
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, PageBreak
styles = getSampleStyleSheet()
sections = [
    ("Field notes from the valley", [
        "These notes collect the observations of the spring survey team. They are written up each evening and checked against the field books the following morning.",
        "The first week was dry and bright. The river ran clear over the gravel beds and the markers were easy to read from the bank.",
    ]),
    ("The second week", [
        "Heavy rain arrived on Tuesday and lasted three days. The water rose quickly and turned brown, carrying branches down from the hills.",
        "On Friday the level at the mill marker reached its highest point of the season. The ferry did not run that afternoon.",
    ]),
    ("The third week", [
        "The water fell slowly through the weekend. New gravel had been laid down below the bridge, and the shape of the bend had changed.",
        "The team agreed to add a new marker at the bend so that the change can be followed next year.",
    ]),
]
def header_footer(c, doc):
    c.saveState()
    c.setFont("Helvetica", 8)
    c.drawString(72, A4[1] - 40, "Valley Field Notes")
    c.drawCentredString(A4[0] / 2, 30, f"Page {doc.page} of 3")
    c.restoreState()
story, md = [], []
for i, (h, paras) in enumerate(sections):
    story.append(Paragraph(h, styles["Heading1"]))
    md.append(f"# {h}\n")
    for p in paras:
        story.append(Paragraph(p, styles["BodyText"]))
        story.append(Spacer(1, 6))
        md.append(p + "\n")
    if i < len(sections) - 1:
        story.append(PageBreak())
SimpleDocTemplate("../fieldnotes.pdf", pagesize=A4, title="Field notes").build(story, onFirstPage=header_footer, onLaterPages=header_footer)
open("fieldnotes.md", "w").write("\n".join(md))
