use super::accessibility::AccessibilityManager;
use super::driver::CdpClient;
use super::AriaElement;
use crate::error::{Result, ToolError};
use serde_json::json;

/// Dispatches DOM interaction commands and returns the updated page snapshot
pub struct BrowserInteractor;

impl BrowserInteractor {
    /// Clicks an element identified by its ARIA reference (@v1:e1)
    pub async fn click_element(
        cdp: &CdpClient,
        target_ref: &str,
        acc_mgr: &mut AccessibilityManager,
    ) -> Result<String> {
        let el = acc_mgr.resolve_ref(target_ref)?.clone();

        tracing::info!(
            target_ref = %target_ref,
            tag = %el.tag,
            name = %el.name,
            "Executing browser click"
        );

        let click_js = build_click_js(&el);
        let exec_res = cdp.evaluate_js(&click_js).await?;

        if exec_res.starts_with("Error:") {
            return Err(ToolError::CommandExec(format!(
                "Failed clicking element '{}' ({}): {}",
                target_ref, el.name, exec_res
            ))
            .into());
        }

        // Allow DOM / network to settle after click
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Advance accessibility revision for subsequent actions
        acc_mgr.next_revision();

        // Retrieve updated HTML and rebuild accessibility tree
        let updated_html = cdp.get_document_html().await.unwrap_or_default();
        let updated_elements = acc_mgr.update_from_html(&updated_html);

        let confirmation = format!(
            "Clicked **{}** `<{}>` \"{}\" (DOM updated to revision v{} with {} interactive elements):\n\n",
            target_ref,
            el.tag,
            el.name,
            acc_mgr.revision(),
            updated_elements.len()
        );

        let report = format_updated_tree(acc_mgr.revision(), &updated_elements);
        Ok(format!("{}{}", confirmation, report))
    }

    /// Types text into an input or textarea element (@v1:e2)
    pub async fn fill_element(
        cdp: &CdpClient,
        target_ref: &str,
        text: &str,
        acc_mgr: &mut AccessibilityManager,
    ) -> Result<String> {
        let el = acc_mgr.resolve_ref(target_ref)?.clone();

        tracing::info!(
            target_ref = %target_ref,
            tag = %el.tag,
            text_len = text.len(),
            "Executing browser text fill"
        );

        let fill_js = build_fill_js(&el, text);
        let exec_res = cdp.evaluate_js(&fill_js).await?;

        if exec_res.starts_with("Error:") {
            return Err(ToolError::CommandExec(format!(
                "Failed filling element '{}' ({}): {}",
                target_ref, el.name, exec_res
            ))
            .into());
        }

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        acc_mgr.next_revision();

        let updated_html = cdp.get_document_html().await.unwrap_or_default();
        let updated_elements = acc_mgr.update_from_html(&updated_html);

        let confirmation = format!(
            "Filled **{}** `<{}>` \"{}\" with \"{}\" (revision v{}):\n\n",
            target_ref,
            el.tag,
            el.name,
            text,
            acc_mgr.revision()
        );

        let report = format_updated_tree(acc_mgr.revision(), &updated_elements);
        Ok(format!("{}{}", confirmation, report))
    }

    /// Scrolls the viewport in the specified direction ("up", "down", "top", "bottom")
    pub async fn scroll_page(cdp: &CdpClient, direction: &str) -> Result<String> {
        let scroll_js = match direction.to_lowercase().as_str() {
            "up" | "pageup" => "window.scrollBy(0, -window.innerHeight * 0.75); 'scrolled_up'",
            "down" | "pagedown" => "window.scrollBy(0, window.innerHeight * 0.75); 'scrolled_down'",
            "top" => "window.scrollTo(0, 0); 'scrolled_top'",
            "bottom" => "window.scrollTo(0, document.body.scrollHeight); 'scrolled_bottom'",
            _ => "window.scrollBy(0, 500); 'scrolled_down'",
        };

        cdp.evaluate_js(scroll_js).await?;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        Ok(format!("Scrolled page {}", direction))
    }
}

fn build_click_js(el: &AriaElement) -> String {
    let tag = &el.tag;
    let name = el.name.replace('"', "\\\"");
    let id_attr = el.attributes.get("id").map(|s| s.as_str()).unwrap_or("");
    let name_attr = el.attributes.get("name").map(|s| s.as_str()).unwrap_or("");
    let href_attr = el.attributes.get("href").map(|s| s.as_str()).unwrap_or("");

    let payload = json!({
        "tag": tag,
        "name": name,
        "id": id_attr,
        "attr_name": name_attr,
        "href": href_attr,
    });

    format!(
        r#"(function() {{
            const p = {};
            const candidates = Array.from(document.querySelectorAll(p.tag));
            let target = candidates.find(el => {{
                if (p.id && el.id === p.id) return true;
                if (p.attr_name && el.name === p.attr_name) return true;
                if (p.href && el.getAttribute('href') === p.href) return true;
                if (el.innerText && el.innerText.trim().includes(p.name)) return true;
                return false;
            }}) || candidates[0];

            if (!target) return 'Error: Element matching tag <' + p.tag + '> not found in DOM';
            target.scrollIntoView({{ behavior: 'instant', block: 'center' }});
            target.focus();
            target.click();
            return 'OK';
        }})()"#,
        payload
    )
}

fn build_fill_js(el: &AriaElement, text: &str) -> String {
    let tag = &el.tag;
    let text_escaped = text.replace('"', "\\\"").replace('\n', "\\n");
    let id_attr = el.attributes.get("id").map(|s| s.as_str()).unwrap_or("");
    let name_attr = el.attributes.get("name").map(|s| s.as_str()).unwrap_or("");
    let placeholder = el
        .attributes
        .get("placeholder")
        .map(|s| s.as_str())
        .unwrap_or("");

    let payload = json!({
        "tag": tag,
        "text": text_escaped,
        "id": id_attr,
        "attr_name": name_attr,
        "placeholder": placeholder,
    });

    format!(
        r#"(function() {{
            const p = {};
            const candidates = Array.from(document.querySelectorAll('input, textarea, [contenteditable="true"]'));
            let target = candidates.find(el => {{
                if (p.id && el.id === p.id) return true;
                if (p.attr_name && el.name === p.attr_name) return true;
                if (p.placeholder && el.getAttribute('placeholder') === p.placeholder) return true;
                return false;
            }}) || candidates[0];

            if (!target) return 'Error: Input element not found in DOM';
            target.scrollIntoView({{ behavior: 'instant', block: 'center' }});
            target.focus();
            target.value = p.text;
            target.dispatchEvent(new Event('input', {{ bubbles: true }}));
            target.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return 'OK';
        }})()"#,
        payload
    )
}

fn format_updated_tree(revision: u32, elements: &[AriaElement]) -> String {
    let mut out = format!("Interactive Accessibility Tree (Revision v{}):\n", revision);
    for el in elements.iter().take(20) {
        out.push_str(&format!(
            "  • **{}** `<{}>` ({}) \"{}\"\n",
            el.ref_id, el.tag, el.role, el.name
        ));
    }
    if elements.len() > 20 {
        out.push_str(&format!(
            "  ... +{} more interactive elements\n",
            elements.len() - 20
        ));
    }
    out
}
