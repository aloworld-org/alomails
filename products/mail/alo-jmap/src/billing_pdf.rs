//! The billing document as a **PDF** (alo Billing, ADR 0035, wave B1.17) —
//! the file a customer is sent, saves and archives.
//!
//! This is the second renderer over [`crate::billing_print::PrintDocument`],
//! and `docs/design/billing.md` records why it is a renderer rather than a
//! converter: we do **not** parse our own HTML back into a PDF. Both this
//! module and the HTML page are handed the same document, the same
//! [`Strings`] table and the same formatters (`amount`, `quantity`, `rate`,
//! `date`, `document_heading`), so the paper and the file cannot disagree
//! about a figure, a date, or what the document is called. Nothing here
//! computes money; it places the store's cents on a sheet.
//!
//! The layout mirrors the print stylesheet's proportions rather than
//! approximating them freehand — the same A4 margins, the same column widths,
//! the same rules — because the two *are* one document, and a customer who has
//! seen one should recognise the other.
//!
//! **What the PDF gives that the HTML page cannot:** the same bytes on every
//! screen and every printer, an attachment for a mail draft (B1.18), and the
//! carrier PDF/A-3 turns into a Factur-X e-invoice (B1.22).

use alo_pdf::{
    Align, Attachment, Canvas, Color, Font, Pdf, PdfDate, Rect, Relationship, TextStyle, mm,
};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use time::{OffsetDateTime, UtcOffset};

use crate::billing_cii as cii;
use crate::billing_einvoice::EInvoice;
use crate::billing_print::{
    PrintDocument, Strings, amount, date, document_heading, monogram, quantity, rate, rate_sentence,
};
use crate::quote_design::ColumnVisibility;
use crate::quote_design_pdf;

// ---- the palette, quoted from the print stylesheet ---------------------------

/// Body text, rules and the total — the document's near-black.
pub(crate) const INK: Color = Color::rgb8(0x16, 0x18, 0x1d);
/// Secondary text: an address under a name, a label beside a figure.
pub(crate) const MUTED: Color = Color::rgb8(0x4a, 0x4f, 0x58);
/// The small-caps labels and the footer.
pub(crate) const FAINT: Color = Color::rgb8(0x6b, 0x72, 0x80);
/// The hairline under a table row.
pub(crate) const HAIRLINE: Color = Color::rgb8(0xdc, 0xdf, 0xe5);

// ---- the sheet ---------------------------------------------------------------

/// Top margin, matching the `@page` rule the print stylesheet sets.
pub(crate) const MARGIN_TOP: f64 = 16.0;
/// Left and right margins.
const MARGIN_SIDE: f64 = 15.0;
/// Bottom margin — where the flow stops and a new page begins.
const MARGIN_BOTTOM: f64 = 14.0;
/// A4 width in millimetres.
const PAGE_WIDTH: f64 = 210.0;
/// A4 height in millimetres.
const PAGE_HEIGHT: f64 = 297.0;
/// Width of the text column.
pub(crate) const COLUMN_WIDTH: f64 = PAGE_WIDTH - 2.0 * MARGIN_SIDE;

/// Body size in points.
pub(crate) const BODY: f64 = 9.5;
/// Body line height — the stylesheet's 1.45.
pub(crate) const LEADING: f64 = BODY * 1.45;
/// Size of the small-caps labels and of the footer.
pub(crate) const SMALL: f64 = 8.0;
/// Line height for [`SMALL`].
pub(crate) const SMALL_LEADING: f64 = SMALL * 1.4;

/// Where a line of `size` must start for its capitals to sit centred in a box.
///
/// [`Canvas::text`] places a baseline one ascent below the `y` it is given, so
/// centring a capital means going *up* from the middle of the box by half a
/// cap height. Used for the monogram and the banner — the only two places
/// text is centred in a box rather than set on a line.
fn cap_centred(box_top: f64, box_height: f64, size: f64) -> f64 {
    box_top + box_height / 2.0 - 0.359 * size
}

/// The document being laid out: the pages finished so far, the page being
/// written, and how far down it the flow has reached.
pub(crate) struct Sheet {
    /// Pages already closed.
    done: Vec<Canvas>,
    /// The page being written.
    pub(crate) page: Canvas,
    /// Top of the next block, in points from the top of the page.
    pub(crate) y: f64,
}

impl Sheet {
    /// A fresh document with one blank page.
    pub(crate) fn new() -> Self {
        Self {
            done: Vec::new(),
            page: Canvas::a4(),
            y: mm(MARGIN_TOP),
        }
    }

    /// Left edge of the text column.
    pub(crate) fn left(&self) -> f64 {
        mm(MARGIN_SIDE)
    }

    /// Right edge of the text column.
    pub(crate) fn right(&self) -> f64 {
        mm(PAGE_WIDTH - MARGIN_SIDE)
    }

    /// The lowest point a block may reach before it belongs on a new page.
    pub(crate) fn floor(&self) -> f64 {
        mm(PAGE_HEIGHT - MARGIN_BOTTOM)
    }

    /// How many pages the sheet holds so far, the one being written included.
    pub(crate) fn page_count(&self) -> usize {
        self.done.len() + 1
    }

    /// Starts a new page.
    pub(crate) fn break_page(&mut self) {
        let finished = std::mem::replace(&mut self.page, Canvas::a4());
        self.done.push(finished);
        self.y = mm(MARGIN_TOP);
    }

    /// Makes room for a block `height` points tall, breaking the page if it
    /// does not fit. Reports whether a break happened, so a caller that has to
    /// repeat something — a table's column headings — knows to.
    ///
    /// A block taller than a whole page is *not* moved to an empty page it
    /// would overflow anyway: it starts where it is. Nothing a document
    /// actually contains is that tall, because the table paginates row by row.
    pub(crate) fn ensure(&mut self, height: f64) -> bool {
        if self.y + height <= self.floor() || self.y <= mm(MARGIN_TOP) {
            return false;
        }
        self.break_page();
        true
    }

    /// Draws one line of text and advances the flow by `leading`.
    pub(crate) fn line(
        &mut self,
        x: f64,
        align: Align,
        style: &TextStyle,
        text: &str,
        leading: f64,
    ) {
        self.page.text(x, self.y, align, style, text);
        self.y += leading;
    }

    /// Draws wrapped text across the whole column, advancing the flow.
    pub(crate) fn paragraph(&mut self, style: &TextStyle, text: &str, leading: f64) {
        let left = self.left();
        for line in style.wrap(text, mm(COLUMN_WIDTH)) {
            self.line(left, Align::Left, style, &line, leading);
        }
    }

    /// Draws a rule across the column at the flow's current position.
    pub(crate) fn rule(&mut self, thickness: f64, color: Color) {
        let (x, y) = (self.left(), self.y);
        self.page.rule(x, y, mm(COLUMN_WIDTH), thickness, color);
    }

    /// Closes the document.
    ///
    /// Page numbers are stamped **here**, once the total is known: `1 / 2` on
    /// a two-page invoice is the difference between a customer who can tell a
    /// page is missing and one who cannot. A one-page document says nothing,
    /// because there is nothing to say.
    fn finish_with(
        mut self,
        title: &str,
        created: PdfDate,
        einvoice: Option<&EInvoice>,
    ) -> Vec<u8> {
        self.done.push(self.page);
        let total = self.done.len();
        let style = TextStyle::new(Font::Regular, SMALL).inked(FAINT);
        let mut pdf = Pdf::new(title, created);
        if let Some(einvoice) = einvoice {
            // The XML is attached, not appended: `/AFRelationship
            // /Alternative` is the statement that these bytes are the *same
            // invoice* in another form, which is precisely what a receiving
            // system checks before trusting them over the page.
            pdf.set_metadata(cii::xmp(einvoice));
            pdf.attach(
                Attachment::new(
                    cii::ATTACHMENT_NAME,
                    cii::ATTACHMENT_MIME,
                    Relationship::Alternative,
                    cii::render(einvoice).into_bytes(),
                    created,
                )
                .described("Factur-X invoice (EN 16931)"),
            );
        }
        for (index, mut page) in self.done.into_iter().enumerate() {
            if total > 1 {
                page.text(
                    mm(PAGE_WIDTH - MARGIN_SIDE),
                    mm(PAGE_HEIGHT - MARGIN_BOTTOM + 4.0),
                    Align::Right,
                    &style,
                    &format!("{} / {total}", index + 1),
                );
            }
            pdf.add_page(page);
        }
        pdf.finish()
    }
}

// ---- the document ------------------------------------------------------------

/// Renders a document as a PDF file.
///
/// `created` stamps the file's metadata and is passed in rather than read
/// here, so one document always renders to the same bytes for the same
/// instant — which is what makes a rendering testable, and a re-download
/// identical to the file the customer already holds.
#[must_use]
pub fn render(doc: &PrintDocument<'_>, s: &Strings, created: PdfDate) -> Vec<u8> {
    render_hybrid(doc, s, created, None)
}

/// Renders the document as a PDF that also **carries the e-invoice** inside it
/// (B1.22), when one could be produced.
///
/// This is what Factur-X is: not a PDF and an XML file sent together, but one
/// file that both a person and a bookkeeping system can read, which is why the
/// two can never be separated in transit or disagree in an archive.
///
/// `einvoice` is `None` when the document has no valid e-invoice — a draft, a
/// quote, or an issuer who has not filled in the details EN 16931 requires —
/// and the answer to that is a perfectly ordinary PDF. **A document must
/// always print**: an invoice that would not render because its XML could not
/// be built would be a worse failure than one that renders without it, and the
/// route that offers the XML on its own ([`crate::billing_invoices`]) is where
/// a tenant is told which rule is unmet.
#[must_use]
pub fn render_hybrid(
    doc: &PrintDocument<'_>,
    s: &Strings,
    created: PdfDate,
    einvoice: Option<&EInvoice>,
) -> Vec<u8> {
    let heading = document_heading(doc, s);
    let mut layout = Layout {
        doc,
        s,
        sheet: Sheet::new(),
    };
    layout.header(&heading);
    layout.banner();
    layout.parties();
    // A quotation's designed content sits around its price table exactly as
    // the studio placed it: what came before the table, then the table and
    // its totals, then the rest (`crate::quote_design_pdf`).
    if let Some(design) = doc.content {
        let (before, _) = design.around_pricing();
        quote_design_pdf::draw(&mut layout.sheet, before, &design.colors);
    }
    layout.lines();
    layout.totals();
    if let Some(design) = doc.content {
        let (_, after) = design.around_pricing();
        quote_design_pdf::draw(&mut layout.sheet, after, &design.colors);
    }
    layout.payment();
    layout.note();
    layout.footer();
    layout.sheet.finish_with(&heading, created, einvoice)
}

/// The document-metadata date for an instant, in UTC.
///
/// Its own function because a route stamps the wall clock and a test stamps a
/// fixed instant, and the two must build it the same way.
#[must_use]
pub fn stamp(at: OffsetDateTime) -> PdfDate {
    let utc = at.to_offset(UtcOffset::UTC);
    PdfDate::new(
        utc.year(),
        u8::from(utc.month()),
        utc.day(),
        utc.hour(),
        utc.minute(),
        utc.second(),
    )
}

/// The name a browser or a mail client should save the document under.
///
/// Built from the heading — the document's own name for itself, so the file on
/// a customer's disk is called what the paper inside it is called — and
/// reduced to characters that are safe in a file name and in a
/// `Content-Disposition` header on every platform. The number is
/// server-generated, but the document's *kind* comes from a translation table,
/// and a table is not a place to trust that nobody ever typed a quote mark.
#[must_use]
pub fn file_name(doc: &PrintDocument<'_>, s: &Strings) -> String {
    let stem = crate::billing_print::file_stem(doc, s);
    if stem.is_empty() {
        "document.pdf".to_owned()
    } else {
        format!("{stem}.pdf")
    }
}

/// Serves a rendered document as a PDF file.
///
/// Three headers, each earning its place:
///
/// - **`Content-Disposition: attachment`**, always. A PDF rendered *inline* is
///   rendered by a viewer inside our own origin, which is a document context we
///   neither wrote nor control; this file exists to be saved, mailed and
///   archived. The name is [`file_name`]'s, which is ASCII alphanumerics and
///   hyphens only, so nothing a customer typed can reach the header's grammar.
/// - **`X-Content-Type-Options: nosniff`**, so nothing re-interprets the bytes
///   as something else.
/// - **`Cache-Control: no-store`**: this is a customer's invoice, not a
///   cacheable asset, and it must not sit in a shared proxy or a disk cache
///   after the session that fetched it has gone.
///
/// There is deliberately **no `Content-Security-Policy`**, unlike the HTML
/// page's response ([`crate::billing_print::response`]): a policy binds a
/// document context, and an attachment is never made into one. `default-src
/// 'none'` on a PDF is a rule that would only ever reach a browser's built-in
/// viewer, which is exactly the path `attachment` already closes.
#[must_use]
pub fn response(bytes: Vec<u8>, file_name: &str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/pdf".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{file_name}\""),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        bytes,
    )
        .into_response()
}

/// Everything one rendering needs: the document, its words, and the sheet.
struct Layout<'a> {
    /// What is being printed.
    doc: &'a PrintDocument<'a>,
    /// The words it is printed in.
    s: &'a Strings,
    /// Where it is being printed.
    sheet: Sheet,
}

/// Body text.
pub(crate) fn body_style() -> TextStyle {
    TextStyle::new(Font::Regular, BODY).inked(INK)
}

/// Secondary body text — an address, a label beside a figure.
pub(crate) fn muted_style() -> TextStyle {
    body_style().inked(MUTED)
}

/// A small-caps section label (`Bill to`, `Payment`).
pub(crate) fn label_style() -> TextStyle {
    TextStyle::new(Font::Regular, SMALL)
        .inked(FAINT)
        .tracked(0.6)
}

impl Layout<'_> {
    /// The issuer block, the title, and the grid of dates beside it.
    fn header(&mut self, heading: &str) {
        let top = self.sheet.y;
        let issuer = self.doc.issuer;
        let (left, right) = (self.sheet.left(), self.sheet.right());
        let (body, muted) = (body_style(), muted_style());

        let issuer_bottom = if issuer.legal_name.is_empty() {
            self.sheet
                .page
                .text(left, top, Align::Left, &muted, self.s.issuer_unstated);
            top + LEADING
        } else {
            let mark = Rect {
                x: left,
                y: top,
                width: mm(13.0),
                height: mm(13.0),
            };
            self.sheet.page.box_filled(mark, mm(2.0), INK);
            self.sheet.page.text(
                mark.x + mark.width / 2.0,
                cap_centred(mark.y, mark.height, 13.0),
                Align::Center,
                &TextStyle::new(Font::Bold, 13.0)
                    .inked(Color::WHITE)
                    .tracked(0.5),
                &monogram(&issuer.legal_name),
            );

            let x = mark.x + mark.width + mm(4.0);
            let mut y = top;
            self.sheet.page.text(
                x,
                y,
                Align::Left,
                &TextStyle::new(Font::Bold, 12.0).inked(INK),
                &issuer.legal_name,
            );
            y += 12.0 * 1.35;
            for text in address_lines(&[
                &issuer.address_line1,
                &issuer.address_line2,
                &format!("{} {}", issuer.postal_code, issuer.city),
                &issuer.country,
            ]) {
                self.sheet.page.text(x, y, Align::Left, &muted, &text);
                y += LEADING;
            }
            y.max(mark.y + mark.height)
        };

        // The title, right-aligned. The number is in it, and deliberately not
        // repeated in the grid below — a document that states its own number
        // twice makes a reader check whether the two agree.
        let mut y = top;
        self.sheet.page.text(
            right,
            y,
            Align::Right,
            &TextStyle::new(Font::Bold, 17.0).inked(INK),
            heading,
        );
        y += 17.0 * 1.25;

        let rows = self.meta_rows();
        let value_column = rows
            .iter()
            .map(|(_, value)| body.width_of(value))
            .fold(0.0_f64, f64::max);
        for (label, value) in &rows {
            self.sheet.page.text(right, y, Align::Right, &body, value);
            self.sheet.page.text(
                right - value_column - mm(4.0),
                y,
                Align::Right,
                &muted,
                label,
            );
            y += LEADING;
        }

        self.sheet.y = issuer_bottom.max(y);
    }

    /// The label/value pairs beside the title: the document's two dates and
    /// the reference it carries. What each one *means* is the document kind's
    /// ([`DocumentKind::primary_date_label`]), so the page and the file cannot
    /// label the same date two different ways.
    fn meta_rows(&self) -> Vec<(&'static str, String)> {
        let kind = self.doc.kind;
        let mut rows: Vec<(&'static str, String)> = Vec::new();
        if let Some(primary) = self.doc.primary_date {
            rows.push((kind.primary_date_label(self.s), date(primary)));
        }
        if let Some(secondary) = self.doc.secondary_date {
            rows.push((kind.secondary_date_label(self.s), date(secondary)));
        }
        if !self.doc.reference.is_empty() {
            rows.push((kind.reference_label(self.s), self.doc.reference.to_owned()));
        }
        rows
    }

    /// The state shouted across the page, when the document is in one.
    fn banner(&mut self) {
        let Some(banner) = self.doc.banner else {
            return;
        };
        let word = banner.word(self.s).to_uppercase();
        let height = LEADING + mm(4.0);
        let (left, width) = (self.sheet.left(), mm(COLUMN_WIDTH));
        self.sheet.y += mm(5.0);
        self.sheet.ensure(height);
        let box_top = self.sheet.y;
        self.sheet.page.box_stroked(
            Rect {
                x: left,
                y: box_top,
                width,
                height,
            },
            mm(1.0),
            mm(0.4),
            INK,
        );
        self.sheet.page.text(
            left + width / 2.0,
            cap_centred(box_top, height, BODY),
            Align::Center,
            &TextStyle::new(Font::Bold, BODY).inked(INK).tracked(1.0),
            &word,
        );
        self.sheet.y = box_top + height;
    }

    /// Who the document is to — a customer, or a supplier on an order.
    fn parties(&mut self) {
        let party = &self.doc.party;
        let (left, body) = (self.sheet.left(), body_style());
        self.sheet.y += mm(7.0);
        self.sheet.line(
            left,
            Align::Left,
            &label_style(),
            self.doc.kind.party_label(self.s),
            SMALL_LEADING + mm(1.5),
        );
        self.sheet.line(
            left,
            Align::Left,
            &TextStyle::new(Font::Bold, BODY).inked(INK),
            party.name,
            LEADING,
        );
        // A domestic address does not print its country; cross-border it is
        // the line that decides the VAT treatment. The printed page's rule.
        let country = if party.country == self.doc.issuer.country {
            ""
        } else {
            party.country
        };
        let mut lines = address_lines(&[
            party.address_line1,
            party.address_line2,
            &format!("{} {}", party.postal_code, party.city),
            country,
        ]);
        if let Some(vat) = party.vat_id.filter(|v| !v.is_empty()) {
            lines.push(format!("{}: {vat}", self.s.customer_vat_id));
        }
        for text in lines {
            self.sheet.line(left, Align::Left, &body, &text, LEADING);
        }
        self.sheet.y += mm(6.0);
    }

    /// The table of what was sold, paginating row by row.
    fn lines(&mut self) {
        let columns = Columns::new(self.sheet.left(), self.sheet.right(), self.visibility());
        self.column_headings(&columns);

        let lines = self.doc.lines;
        if lines.is_empty() {
            let (left, muted) = (self.sheet.left(), muted_style());
            let empty = self.s.no_lines;
            self.sheet.y += mm(2.0);
            self.sheet.line(left, Align::Left, &muted, empty, LEADING);
            self.sheet.y += mm(2.0);
            return;
        }

        let body = body_style();
        for line in lines {
            let wrapped = body.wrap(&line.description, columns.description_width);
            let height = wrapped.len() as f64 * LEADING + mm(3.6);
            // A row never straddles two pages, and a continuation page repeats
            // the headings — a column of figures with nothing above it is a
            // column a reader has to guess at.
            if self.sheet.ensure(height + mm(6.0)) {
                self.column_headings(&columns);
            }
            let top = self.sheet.y + mm(1.8);
            for (index, text) in wrapped.iter().enumerate() {
                self.sheet.page.text(
                    columns.description,
                    top + index as f64 * LEADING,
                    Align::Left,
                    &body,
                    text,
                );
            }
            let unit = if line.unit.is_empty() || !columns.unit {
                String::new()
            } else {
                format!(" {}", line.unit)
            };
            let figures = [
                (
                    columns.quantity,
                    format!("{}{unit}", quantity(line.qty_milli, self.s)),
                ),
                (columns.unit_price, amount(line.unit_price_cents, self.s)),
                (columns.vat, rate(line.vat_rate_bp, self.s)),
                (columns.net, amount(line.net_cents(), self.s)),
            ];
            for (x, text) in figures {
                // A hidden column has no edge to align on.
                let Some(x) = x else { continue };
                self.sheet
                    .page
                    .text(x - mm(CELL_PADDING), top, Align::Right, &body, &text);
            }
            self.sheet.y += height;
            self.sheet.rule(mm(0.2), HAIRLINE);
        }
    }

    /// Which price-table columns the document prints — the studio's choice on
    /// a designed quotation, everything on any other document.
    fn visibility(&self) -> ColumnVisibility {
        self.doc
            .content
            .map_or_else(ColumnVisibility::default, |design| design.columns)
    }

    /// The heading row of the table, and the rule under it.
    fn column_headings(&mut self, columns: &Columns) {
        let style = TextStyle::new(Font::Bold, SMALL).inked(INK).tracked(0.5);
        let headings = [
            (columns.quantity, self.s.quantity),
            (columns.unit_price, self.s.unit_price),
            (columns.vat, self.s.vat_rate),
            (columns.net, self.s.line_net),
        ];
        let description = self.s.description.to_uppercase();
        let top = self.sheet.y + mm(1.8);
        self.sheet
            .page
            .text(columns.description, top, Align::Left, &style, &description);
        for (x, text) in headings {
            let Some(x) = x else { continue };
            self.sheet.page.text(
                x - mm(CELL_PADDING),
                top,
                Align::Right,
                &style,
                &text.to_uppercase(),
            );
        }
        self.sheet.y = top + SMALL_LEADING + mm(1.8);
        self.sheet.rule(mm(0.5), INK);
    }

    /// What the document comes to — the server's figures, per VAT rate and in
    /// total.
    fn totals(&mut self) {
        let currency = self.doc.currency.to_owned();
        let mut rows: Vec<(String, i64)> =
            vec![(self.s.net_total.to_owned(), self.doc.totals.net_cents)];
        for subtotal in &self.doc.totals.vat_by_rate {
            rows.push((
                (self.s.vat_at)(&rate(subtotal.rate_bp, self.s)),
                subtotal.vat_cents,
            ));
        }
        let gross = amount(self.doc.totals.gross_cents, self.s);
        let grand_label = self.s.gross_total;
        // The restatement follows the grand total, as on the printed page: the
        // VAT in the issuer's own currency, then the rate it was converted at
        // (`crate::billing_print::Restated` — required on a foreign-currency
        // document, not decoration).
        let restated = self.doc.restated.as_ref().map(|r| {
            (
                (self.s.vat_in)(&r.currency),
                format!("{} {}", r.currency, amount(r.vat_cents, self.s)),
                (self.s.converted_at)(&rate_sentence(r, self.doc.currency), &date(r.rate_date)),
            )
        });

        let block = self.sheet.right() - mm(78.0);
        let (left, right) = (block + mm(2.0), self.sheet.right() - mm(2.0));
        let extra = if restated.is_some() { 2 } else { 0 };
        let height = (rows.len() + 1 + extra) as f64 * (LEADING + mm(2.4)) + mm(6.0);
        self.sheet.y += mm(4.0);
        self.sheet.ensure(height);

        let (body, muted) = (body_style(), muted_style());
        for (label, cents) in &rows {
            let y = self.sheet.y + mm(1.2);
            self.sheet.page.text(left, y, Align::Left, &muted, label);
            self.sheet.page.text(
                right,
                y,
                Align::Right,
                &body,
                &format!("{currency} {}", amount(*cents, self.s)),
            );
            self.sheet.y = y + LEADING + mm(1.2);
        }

        self.sheet
            .page
            .rule(block, self.sheet.y, mm(78.0), mm(0.5), INK);
        let grand = TextStyle::new(Font::Bold, 11.0).inked(INK);
        let y = self.sheet.y + mm(2.0);
        self.sheet
            .page
            .text(left, y, Align::Left, &grand, grand_label);
        self.sheet.page.text(
            right,
            y,
            Align::Right,
            &grand,
            &format!("{currency} {gross}"),
        );
        self.sheet.y = y + 11.0 * 1.45;

        if let Some((label, value, sentence)) = restated {
            let y = self.sheet.y + mm(1.2);
            self.sheet.page.text(left, y, Align::Left, &muted, &label);
            self.sheet.page.text(right, y, Align::Right, &body, &value);
            self.sheet.y = y + LEADING + mm(1.2);
            // The rate itself, small and right-aligned under the block, exactly
            // where the HTML page puts it.
            let small = TextStyle::new(Font::Regular, 8.0).inked(MUTED);
            for line in small.wrap(&sentence, mm(78.0) - mm(4.0)) {
                self.sheet
                    .page
                    .text(right, self.sheet.y, Align::Right, &small, &line);
                self.sheet.y += SMALL_LEADING;
            }
        }
    }

    /// What happens about the money, and where it goes.
    fn payment(&mut self) {
        let sentence = self.payment_sentence();
        let fields = self.bank_fields();
        let (body, keys) = (body_style(), label_style());
        let sentence_lines = body.wrap(&sentence, mm(COLUMN_WIDTH)).len();
        let bank_height = if fields.is_empty() {
            0.0
        } else {
            SMALL_LEADING + LEADING + mm(2.0)
        };
        let height = SMALL_LEADING + sentence_lines as f64 * LEADING + bank_height;
        let (left, right) = (self.sheet.left(), self.sheet.right());

        self.sheet.y += mm(7.0);
        self.sheet.ensure(height);
        let closing = self.doc.kind.closing_label(self.s);
        self.sheet
            .line(left, Align::Left, &keys, closing, SMALL_LEADING + mm(1.5));
        self.sheet.paragraph(&body, &sentence, LEADING);
        if fields.is_empty() {
            return;
        }
        self.sheet.y += mm(2.0);

        // The account block reads left to right, each field a small-caps key
        // over its value, wrapping onto a second row rather than off the page.
        let mut x = left;
        let mut row_top = self.sheet.y;
        for (key, value) in fields {
            let key = key.to_uppercase();
            let column = keys.width_of(&key).max(body.width_of(&value));
            if x > left && x + column > right {
                x = left;
                row_top += SMALL_LEADING + LEADING + mm(2.0);
            }
            self.sheet.page.text(x, row_top, Align::Left, &keys, &key);
            self.sheet
                .page
                .text(x, row_top + SMALL_LEADING, Align::Left, &body, &value);
            x += column + mm(8.0);
        }
        self.sheet.y = row_top + SMALL_LEADING + LEADING;
    }

    /// The sentence under the closing label — the one that says whether
    /// anything is owed at all, or when the goods are wanted.
    ///
    /// The page's own ([`crate::billing_print::closing_sentence`]): the file
    /// and the paper are one document, and a sentence written twice is a
    /// sentence that eventually says two things.
    fn payment_sentence(&self) -> String {
        crate::billing_print::closing_sentence(self.doc, self.s)
    }

    /// The account the money goes to — on an invoice only
    /// ([`DocumentKind::prints_bank_details`]).
    ///
    /// A quote is not paid, a credit note is not paid *to* us, and an order we
    /// placed is not paid to us either, so none of them prints an account: an
    /// IBAN under "nothing is payable" is exactly how a document gets paid
    /// twice. The printed page's rule, held here too.
    fn bank_fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = Vec::new();
        if !self.doc.kind.prints_bank_details() {
            return fields;
        }
        let issuer = self.doc.issuer;
        if let Some(iban) = issuer.iban.as_deref().filter(|v| !v.is_empty()) {
            fields.push((self.s.iban, alo_store::iban::grouped(iban)));
        }
        if let Some(bic) = issuer.bic.as_deref().filter(|v| !v.is_empty()) {
            fields.push((self.s.bic, bic.to_owned()));
        }
        if fields.is_empty() {
            return fields;
        }
        let holder = issuer.effective_account_holder();
        if !holder.is_empty() {
            let holder = if issuer.bank_name.is_empty() {
                holder.to_owned()
            } else {
                format!("{holder} \u{b7} {}", issuer.bank_name)
            };
            fields.push((self.s.account_holder, holder));
        }
        fields
    }

    /// The note typed on the document, if there is one.
    fn note(&mut self) {
        if self.doc.note.is_empty() {
            return;
        }
        let body = body_style();
        let note = self.doc.note;
        let height = body.wrap(note, mm(COLUMN_WIDTH)).len() as f64 * LEADING;
        self.sheet.y += mm(6.0);
        self.sheet.ensure(height);
        self.sheet.paragraph(&body, note, LEADING);
    }

    /// Who the issuer is, in law: the identifiers a document has to carry.
    fn footer(&mut self) {
        let issuer = self.doc.issuer;
        let mut parts: Vec<String> = Vec::new();
        if let Some(vat_id) = issuer.vat_id.as_deref().filter(|v| !v.is_empty()) {
            parts.push(format!("{}: {vat_id}", self.s.vat_id));
        }
        if !issuer.registration_no.is_empty() {
            parts.push(format!(
                "{}: {}",
                self.s.registration_no, issuer.registration_no
            ));
        }
        for contact in [&issuer.email, &issuer.phone, &issuer.website] {
            if !contact.is_empty() {
                parts.push(contact.clone());
            }
        }
        if parts.is_empty() && issuer.footer_note.is_empty() {
            return;
        }

        let small = TextStyle::new(Font::Regular, SMALL).inked(FAINT);
        let joined = parts.join(" \u{b7} ");
        let note = issuer.footer_note.clone();
        let height = mm(3.0)
            + (small.wrap(&joined, mm(COLUMN_WIDTH)).len()
                + small.wrap(&note, mm(COLUMN_WIDTH)).len()) as f64
                * SMALL_LEADING;
        self.sheet.y += mm(9.0);
        self.sheet.ensure(height);
        self.sheet.rule(mm(0.2), HAIRLINE);
        self.sheet.y += mm(3.0);
        for text in [&joined, &note] {
            if !text.is_empty() {
                self.sheet.paragraph(&small, text, SMALL_LEADING);
            }
        }
    }
}

/// Padding inside a table cell, on each side.
const CELL_PADDING: f64 = 2.0;

/// Where each column of the table of lines sits.
///
/// The numeric columns are named by their **right** edge, because that is
/// what a column of figures lines up on, and a column the design hides has
/// no edge at all (`None`): the description takes what is left.
struct Columns {
    /// Right edge of the line's net amount.
    net: Option<f64>,
    /// Right edge of the VAT rate.
    vat: Option<f64>,
    /// Right edge of the unit price.
    unit_price: Option<f64>,
    /// Right edge of the quantity.
    quantity: Option<f64>,
    /// Whether the unit label is printed beside the quantity.
    unit: bool,
    /// Left edge of the description.
    description: f64,
    /// How wide a description may be before it wraps.
    description_width: f64,
}

impl Columns {
    /// The column geometry for a text column between `left` and `right`,
    /// with the hidden columns' room given to the description.
    ///
    /// Right to left, the widths are net 26, VAT 16, unit price 26 and
    /// quantity 22 mm — the print stylesheet's proportions.
    fn new(left: f64, right: f64, visible: ColumnVisibility) -> Self {
        let mut edge = right;
        let mut place = |shown: bool, width_mm: f64| -> Option<f64> {
            if !shown {
                return None;
            }
            let at = edge;
            edge -= mm(width_mm);
            Some(at)
        };
        let net = place(visible.net, 26.0);
        let vat = place(visible.vat, 16.0);
        let unit_price = place(visible.unit_price, 26.0);
        let quantity = place(visible.quantity, 22.0);
        let figures = right - edge;
        Self {
            net,
            vat,
            unit_price,
            quantity,
            unit: visible.unit,
            description: left,
            // Wide enough to fill the cell, and never a millimetre wider: a
            // description that ran under the quantity beside it would print
            // two numbers on top of each other.
            description_width: mm(COLUMN_WIDTH) - figures - 2.0 * mm(CELL_PADDING),
        }
    }
}

/// The non-empty parts of an address, trimmed — the printed page's
/// `address_lines`, without the markup.
fn address_lines(parts: &[&str]) -> Vec<String> {
    parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use alo_store::billing_settings::BillingSettings;
    use alo_store::billing_totals::{LineFigures, Totals, totals};
    use alo_store::{BillingCustomerId, BillingLineId, Customer, Line};
    use time::{Date, Month, OffsetDateTime};

    use crate::billing_print::{Banner, DocumentKind, Party as PrintParty, strings_for};

    /// A calendar date without a macro (`time` is built here without its
    /// `macros` feature) and without an `unwrap` (denied workspace-wide).
    fn day(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap_or(Month::January), day)
            .unwrap_or(Date::MIN)
    }

    fn customer() -> Customer {
        Customer {
            id: BillingCustomerId::new("cus-1".to_owned()),
            name: "Kunde & Söhne GmbH".to_owned(),
            address_line1: "Hauptstraße 5".to_owned(),
            address_line2: String::new(),
            postal_code: "10115".to_owned(),
            city: "Berlin".to_owned(),
            country: "DE".to_owned(),
            vat_id: Some("DE811907980".to_owned()),
            email: None,
            payment_terms_days: 14,
            currency: "EUR".to_owned(),
            contact_id: None,
            archived_at: None,
            created_by: "u1".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn issuer() -> BillingSettings {
        BillingSettings {
            legal_name: "Alo Werkplaats B.V.".to_owned(),
            address_line1: "Keizersgracht 1".to_owned(),
            postal_code: "1015 CJ".to_owned(),
            city: "Amsterdam".to_owned(),
            country: "NL".to_owned(),
            vat_id: Some("NL812345678B01".to_owned()),
            registration_no: "KVK 90123456".to_owned(),
            email: "billing@alo.test".to_owned(),
            iban: Some("NL91ABNA0417164300".to_owned()),
            bic: Some("ABNANL2A".to_owned()),
            bank_name: "ABN AMRO".to_owned(),
            footer_note: "Retention of title until paid in full.".to_owned(),
            updated_by: Some("u1".to_owned()),
            updated_at: Some(OffsetDateTime::UNIX_EPOCH),
            ..Default::default()
        }
    }

    fn line(description: &str, qty_milli: i64, unit_price_cents: i64, vat_rate_bp: i32) -> Line {
        Line {
            id: BillingLineId::new(format!("l-{description}")),
            line_order: 0,
            description: description.to_owned(),
            unit: "hour".to_owned(),
            qty_milli,
            unit_price_cents,
            vat_rate_bp,
        }
    }

    fn figures(lines: &[Line]) -> Totals {
        totals(
            &lines
                .iter()
                .map(|l| LineFigures {
                    qty_milli: l.qty_milli,
                    unit_price_cents: l.unit_price_cents,
                    vat_rate_bp: l.vat_rate_bp,
                })
                .collect::<Vec<_>>(),
        )
    }

    fn invoice<'a>(
        customer: &'a Customer,
        issuer: &'a BillingSettings,
        lines: &'a [Line],
        totals: &'a Totals,
    ) -> PrintDocument<'a> {
        PrintDocument {
            kind: DocumentKind::Invoice,
            banner: None,
            number: Some("INV-2026-00001"),
            primary_date: Some(day(2026, 8, 7)),
            secondary_date: Some(day(2026, 8, 21)),
            reference: "PO-42",
            note: "Thank you.",
            currency: "EUR",
            payment_terms_days: Some(14),
            credits_number: None,
            party: PrintParty::customer(customer),
            lines,
            totals,
            restated: None,
            issuer,
            content: None,
        }
    }

    /// A fixed instant, so a rendering is a value and not a moment.
    fn created() -> PdfDate {
        PdfDate::new(2026, 8, 7, 9, 30, 0)
    }

    fn bytes(doc: &PrintDocument<'_>) -> Vec<u8> {
        render(doc, strings_for("en"), created())
    }

    /// The document as an independent PDF parser reads it.
    ///
    /// Deliberately not a search through our own content stream: what matters
    /// is what a *reader* sees, encoding and all, so the assertions below go
    /// through a parser that knows nothing about how the file was written.
    fn text(doc: &PrintDocument<'_>) -> String {
        let file = bytes(doc);
        assert!(file.starts_with(b"%PDF-1.7"), "not a PDF");
        let read = pdf_extract::extract_text_from_mem(&file)
            .unwrap_or_else(|e| panic!("our own PDF could not be read back: {e}"));
        // A grouped amount separates its digits with a no-break space, which
        // is the point; whether an extractor hands it back as U+00A0 or as an
        // ordinary space is the extractor's business, not the document's.
        read.replace('\u{a0}', " ")
    }

    /// How many pages the rendered file has, counted in the file itself.
    fn pages(doc: &PrintDocument<'_>) -> usize {
        String::from_utf8_lossy(&bytes(doc))
            .matches("/Type /Page ")
            .count()
    }

    #[test]
    fn an_invoice_carries_the_stores_own_figures_onto_the_page() {
        let (c, i) = (customer(), issuer());
        let lines = vec![
            line("Consulting", 2_000, 12_000, 2100),
            line("Printed handbook", 2_000, 4_500, 900),
            line("Goodwill discount", -500, 12_000, 2100),
        ];
        let t = figures(&lines);
        let read = text(&invoice(&c, &i, &lines, &t));

        // Both parties, in full.
        assert!(read.contains("Invoice INV-2026-00001"));
        assert!(read.contains("Kunde & Söhne GmbH"), "{read}");
        assert!(read.contains("Hauptstraße 5") && read.contains("10115 Berlin"));
        assert!(read.contains("Alo Werkplaats B.V."));
        // Cross-border, so the customer's country is on the address.
        assert!(read.contains("DE811907980") && read.contains("NL812345678B01"));
        // The dates, ISO, and the customer's own reference.
        assert!(read.contains("2026-08-07") && read.contains("2026-08-21"));
        assert!(read.contains("PO-42"));
        // The money is the store's, to the cent — 240.00 + 90.00 − 60.00 net,
        // VAT of 37.80 at 21% and 8.10 at 9%.
        assert_eq!(t.net_cents, 27_000);
        assert!(read.contains("EUR 270.00"), "net total missing: {read}");
        assert!(read.contains("VAT 21%") && read.contains("VAT 9%"));
        assert!(
            read.contains("EUR 37.80") && read.contains("EUR 8.10"),
            "{read}"
        );
        assert!(read.contains("EUR 315.90"), "gross: {read}");
        // …and the account it is paid into, grouped as it is read aloud.
        assert!(read.contains("NL91 ABNA 0417 1643 00"));
        assert!(read.contains("Payable by 2026-08-21"));
        assert!(read.contains("Retention of title"));
    }

    #[test]
    fn every_figure_on_the_page_is_the_one_the_formatter_produced() {
        // The whole point of the second renderer sharing the first's
        // formatters: no number is formatted twice, in two ways.
        let (c, i) = (customer(), issuer());
        let lines = vec![line("Consulting", 1_500, 99_999, 2100)];
        let t = figures(&lines);
        let s = strings_for("en");
        let read = text(&invoice(&c, &i, &lines, &t));
        for figure in [
            amount(t.net_cents, s),
            amount(t.gross_cents, s),
            amount(lines[0].unit_price_cents, s),
            quantity(lines[0].qty_milli, s),
            rate(lines[0].vat_rate_bp, s),
        ] {
            // The narrow no-break space a grouped amount carries prints as a
            // no-break space; everything else is unchanged.
            let printed = figure.replace('\u{202f}', " ");
            assert!(read.contains(&printed), "{printed:?} is not on the page");
        }
    }

    #[test]
    fn a_draft_says_so_and_carries_no_number() {
        let (c, i) = (customer(), issuer());
        let lines = vec![line("Consulting", 2_000, 12_000, 2100)];
        let t = figures(&lines);
        let doc = PrintDocument {
            banner: Some(Banner::Draft),
            number: None,
            primary_date: None,
            secondary_date: None,
            ..invoice(&c, &i, &lines, &t)
        };
        let read = text(&doc);
        assert!(read.contains("DRAFT"));
        assert!(!read.contains("INV-2026"), "a draft must print no number");
        // With no due date it states the term, so the page never simply omits
        // when the money is owed.
        assert!(read.contains("within 14 days"));
        assert_eq!(file_name(&doc, strings_for("en")), "Invoice.pdf");
    }

    #[test]
    fn a_void_invoice_keeps_its_number_and_says_it_is_void() {
        let (c, i) = (customer(), issuer());
        let lines = vec![line("Consulting", 2_000, 12_000, 2100)];
        let t = figures(&lines);
        let read = text(&PrintDocument {
            banner: Some(Banner::Void),
            ..invoice(&c, &i, &lines, &t)
        });
        assert!(read.contains("VOID") && read.contains("INV-2026-00001"));
    }

    #[test]
    fn a_credit_note_names_what_it_corrects_and_prints_no_account() {
        let (c, i) = (customer(), issuer());
        let lines = vec![line("Consulting", -2_000, 12_000, 2100)];
        let t = figures(&lines);
        let doc = PrintDocument {
            kind: DocumentKind::CreditNote,
            number: Some("INV-2026-00002"),
            credits_number: Some("INV-2026-00001"),
            ..invoice(&c, &i, &lines, &t)
        };
        let read = text(&doc);
        assert!(read.contains("Credit note INV-2026-00002"));
        assert!(read.contains("corrects invoice INV-2026-00001"));
        assert!(read.contains("nothing is payable"));
        // An IBAN under "nothing is payable" is how a document gets paid twice.
        assert!(!read.contains("NL91 ABNA"), "{read}");
        assert!(read.contains("290.40"), "the money is negative: {read}");
        assert_eq!(
            file_name(&doc, strings_for("en")),
            "Credit-note-INV-2026-00002.pdf"
        );
    }

    #[test]
    fn a_quote_is_dated_as_an_offer_and_owes_nothing() {
        let (c, i) = (customer(), issuer());
        let lines = vec![line("Consulting", 2_000, 12_000, 2100)];
        let t = figures(&lines);
        let doc = PrintDocument {
            kind: DocumentKind::Quote,
            number: Some("QUO-2026-00001"),
            ..invoice(&c, &i, &lines, &t)
        };
        let read = text(&doc);
        assert!(read.contains("Quote QUO-2026-00001"));
        assert!(read.contains("Sent") && read.contains("Valid until"));
        assert!(!read.contains("Due date"));
        assert!(read.contains("stands until 2026-08-21"));
        assert!(!read.contains("NL91 ABNA"));
    }

    #[test]
    fn a_long_document_paginates_and_never_leaves_a_column_unlabelled() {
        let (c, i) = (customer(), issuer());
        let lines: Vec<Line> = (0..40)
            .map(|n| line(&format!("Line item number {n}"), 1_000, 12_345, 2100))
            .collect();
        let t = figures(&lines);
        let doc = invoice(&c, &i, &lines, &t);

        let page_count = pages(&doc);
        assert!(page_count > 1, "40 lines must not fit on one page");
        let read = text(&doc);
        // Every line reached the paper — none was silently dropped at a break.
        for n in 0..40 {
            assert!(read.contains(&format!("Line item number {n}")), "line {n}");
        }
        // The column headings repeat on every page, and the pages say how many
        // they are, so a reader can tell one is missing.
        assert_eq!(read.matches("DESCRIPTION").count(), page_count);
        assert!(read.contains(&format!("1 / {page_count}")));
        assert!(read.contains(&format!("{page_count} / {page_count}")));
        // The totals are still the store's after all that pagination.
        let gross = amount(t.gross_cents, strings_for("en")).replace('\u{202f}', " ");
        assert!(
            read.contains(&format!("EUR {gross}")),
            "gross after {page_count} pages: {read}"
        );
    }

    #[test]
    fn a_one_page_document_does_not_number_its_pages() {
        let (c, i) = (customer(), issuer());
        let lines = vec![line("Consulting", 2_000, 12_000, 2100)];
        let t = figures(&lines);
        let doc = invoice(&c, &i, &lines, &t);
        assert_eq!(pages(&doc), 1);
        assert!(!text(&doc).contains("1 / 1"), "nothing to say on one page");
    }

    #[test]
    fn nothing_a_customer_typed_can_become_pdf_structure() {
        // The name closes a PDF string, opens a dictionary and escapes the
        // escape. If any of it reached the file unescaped, the parser below
        // would fail — which is the assertion.
        let mut c = customer();
        c.name = "Acme (Holdings) \\ >> endobj Ltd".to_owned();
        c.city = "Berlin ) Tj".to_owned();
        let i = issuer();
        let lines = vec![line("Consulting ( \\ )", 2_000, 12_000, 2100)];
        let t = figures(&lines);
        let read = text(&invoice(&c, &i, &lines, &t));
        assert!(read.contains("Acme (Holdings) \\ >> endobj Ltd"), "{read}");
        assert!(read.contains("Berlin ) Tj"));
        assert!(read.contains("Consulting ( \\ )"));
    }

    #[test]
    fn a_name_the_standard_fonts_cannot_spell_still_prints_legibly() {
        // The documented cost of not embedding a font (B1.22 removes it): a
        // Polish name is folded to its base letters, never dropped and never
        // turned into a row of question marks.
        let mut c = customer();
        c.name = "Łukasz Wójcik sp. z o.o.".to_owned();
        c.city = "Kraków".to_owned();
        let i = issuer();
        let lines = vec![line("Consulting", 2_000, 12_000, 2100)];
        let t = figures(&lines);
        let read = text(&invoice(&c, &i, &lines, &t));
        assert!(read.contains("Lukasz Wójcik sp. z o.o."), "{read}");
        assert!(read.contains("Kraków"), "ó is in the repertoire");
    }

    #[test]
    fn a_tenant_that_has_not_filled_its_details_in_still_gets_a_document() {
        let c = customer();
        let i = BillingSettings::default();
        let lines = vec![line("Consulting", 2_000, 12_000, 2100)];
        let t = figures(&lines);
        let read = text(&invoice(&c, &i, &lines, &t));
        // It prints, it says what is missing, and it invents nothing.
        assert!(read.contains("have not been filled in yet"));
        assert!(read.contains("EUR 290.40"));
        assert!(!read.contains("IBAN"), "no account, not a placeholder one");
    }

    #[test]
    fn an_empty_document_says_it_is_empty_rather_than_showing_a_bare_table() {
        let (c, i) = (customer(), issuer());
        let lines: Vec<Line> = Vec::new();
        let t = figures(&lines);
        let read = text(&invoice(&c, &i, &lines, &t));
        assert!(read.contains("no lines yet"));
        assert!(read.contains("EUR 0.00"));
    }

    #[test]
    fn the_same_document_always_renders_to_the_same_bytes() {
        // What makes a re-download identical to the file the customer already
        // holds — and what would be impossible if the renderer read a clock.
        let (c, i) = (customer(), issuer());
        let lines = vec![line("Consulting", 2_000, 12_000, 2100)];
        let t = figures(&lines);
        let doc = invoice(&c, &i, &lines, &t);
        assert_eq!(bytes(&doc), bytes(&doc));
        // A different instant is a different file, and both are still valid.
        let later = render(&doc, strings_for("en"), PdfDate::new(2026, 8, 7, 9, 30, 1));
        assert_ne!(bytes(&doc), later);
        assert!(later.starts_with(b"%PDF-1.7") && later.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn a_file_name_is_the_documents_own_name_and_nothing_else() {
        let (c, i) = (customer(), issuer());
        let lines = vec![line("Consulting", 2_000, 12_000, 2100)];
        let t = figures(&lines);
        let s = strings_for("en");
        assert_eq!(
            file_name(&invoice(&c, &i, &lines, &t), s),
            "Invoice-INV-2026-00001.pdf"
        );
        // Nothing that is not a letter, a digit or a separator survives, so a
        // name can never reach the grammar of a Content-Disposition header.
        let doc = PrintDocument {
            number: Some("INV \"2026\"; rm -rf /\r\n"),
            ..invoice(&c, &i, &lines, &t)
        };
        let name = file_name(&doc, s);
        assert_eq!(name, "Invoice-INV-2026-rm-rf.pdf");
        assert!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        );
    }

    #[test]
    fn the_response_is_a_download_and_not_a_cacheable_asset() {
        let response = response(b"%PDF-1.7\n".to_vec(), "Invoice-INV-2026-00001.pdf");
        let headers = response.headers();
        assert_eq!(headers["content-type"], "application/pdf");
        assert_eq!(
            headers["content-disposition"],
            "attachment; filename=\"Invoice-INV-2026-00001.pdf\""
        );
        assert_eq!(headers["cache-control"], "no-store");
        assert_eq!(headers["x-content-type-options"], "nosniff");
    }

    #[test]
    fn the_metadata_stamp_is_the_instant_it_was_given_in_utc() {
        // A route stamps the wall clock and a test stamps a fixed instant;
        // both must land on the same UTC calendar.
        let at = OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(1_775_000_000);
        assert_eq!(stamp(at), stamp(at.to_offset(UtcOffset::UTC)));
        let east = at.to_offset(UtcOffset::from_hms(5, 30, 0).unwrap_or(UtcOffset::UTC));
        assert_eq!(stamp(east), stamp(at), "the zone is not the instant");
    }
}
