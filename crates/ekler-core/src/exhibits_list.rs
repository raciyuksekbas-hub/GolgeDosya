use crate::model::Project;

/// Otomatik "EKLER LİSTESİ" üretici
///
/// Örnek Çıktı:
/// EKLER
/// Ek-1: İş Sözleşmesi — 6 sayfa
/// Ek-2: İhtarname — 3 sayfa
/// Ek-3: Banka Dekontları — 14 sayfa
pub struct ExhibitsListGenerator;

impl ExhibitsListGenerator {
    pub fn generate_plain_text(
        project: &Project,
        exhibit_page_counts: &[(String, usize)],
    ) -> String {
        let mut out = String::from("EKLER\n\n");
        for exhibit in &project.exhibits {
            let pages = exhibit_page_counts
                .iter()
                .find(|(id, _)| id == &exhibit.id)
                .map(|(_, c)| *c)
                .unwrap_or(0);

            out.push_str(&format!(
                "Ek-{}: {} — {} sayfa\n",
                exhibit.order, exhibit.name, pages
            ));
        }
        out
    }

    pub fn generate_markdown(project: &Project, exhibit_page_counts: &[(String, usize)]) -> String {
        let mut out = String::from("### EKLER\n\n");
        for exhibit in &project.exhibits {
            let pages = exhibit_page_counts
                .iter()
                .find(|(id, _)| id == &exhibit.id)
                .map(|(_, c)| *c)
                .unwrap_or(0);

            out.push_str(&format!(
                "* **Ek-{}**: {} — *{} sayfa*\n",
                exhibit.order, exhibit.name, pages
            ));
        }
        out
    }
}
