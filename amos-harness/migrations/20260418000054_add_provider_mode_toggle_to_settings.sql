-- Add a provider-mode toggle to the top of the system-settings canvas.
--
-- Before this migration the canvas only managed BYOK provider configs,
-- leaving the `llm_provider_mode` setting unreachable from the UI. A
-- customer adding an Anthropic key and clicking "Activate" thought they'd
-- switched to BYOK, but the agent_proxy was still routing through shared
-- Bedrock because the mode setting hadn't changed.
--
-- The activate endpoint now auto-flips the mode to "byok" (server-side
-- fix in routes/llm_providers.rs). This migration adds the explicit UI
-- toggle so customers can switch back to shared Bedrock, or see at a
-- glance which mode they're in.

UPDATE canvases SET
    html_content = REPLACE(
        html_content,
        '<h2 class="mb-4">Settings</h2>',
        '<h2 class="mb-4">Settings</h2>

    <!-- ── Provider Mode Toggle ── -->
    <div class="card mb-4" id="providerModeCard" style="display:none">
        <div class="card-header">
            <h5 class="mb-0"><i data-lucide="settings" style="width:18px;height:18px;display:inline-block;vertical-align:middle;margin-right:6px"></i> AI Provider Mode</h5>
        </div>
        <div class="card-body">
            <p class="text-muted small mb-3">Choose which AI backs your chats. You can switch anytime. Your BYOK keys and shared-Bedrock pricing stay configured in the background either way.</p>
            <div class="form-check mb-2">
                <input class="form-check-input" type="radio" name="providerMode" id="modeSharedBedrock" value="shared_bedrock" onchange="onModeChange(this.value)">
                <label class="form-check-label" for="modeSharedBedrock">
                    <strong>Shared Bedrock (AMOS-hosted)</strong>
                    <div class="small text-muted">AMOS provides the AWS Bedrock access. Usage billed at Bedrock pricing + 3%.</div>
                </label>
            </div>
            <div class="form-check">
                <input class="form-check-input" type="radio" name="providerMode" id="modeByok" value="byok" onchange="onModeChange(this.value)">
                <label class="form-check-label" for="modeByok">
                    <strong>Bring Your Own Key (BYOK)</strong>
                    <div class="small text-muted">Use your own Anthropic, OpenAI, or custom provider API key. Usage billed directly to your API account.</div>
                </label>
            </div>
            <div id="modeStatus" class="small mt-3"></div>
        </div>
    </div>'
    ),
    js_content = REPLACE(
        js_content,
        'document.addEventListener("DOMContentLoaded", function() {
    loadProviders();
});',
        'document.addEventListener("DOMContentLoaded", function() {
    loadSettings();
    loadProviders();
});

async function loadSettings() {
    try {
        var resp = await fetch("/api/v1/settings");
        if (!resp.ok) return;
        var s = await resp.json();
        var card = document.getElementById("providerModeCard");
        // Hide the whole toggle on self-hosted — BYOK is the only option.
        if (!s.shared_bedrock_available) {
            card.style.display = "none";
            return;
        }
        card.style.display = "";
        var mode = s.llm_provider_mode || "shared_bedrock";
        var radio = document.getElementById(mode === "byok" ? "modeByok" : "modeSharedBedrock");
        if (radio) radio.checked = true;
        updateModeStatus(mode);
    } catch(e) { /* non-fatal */ }
}

async function onModeChange(mode) {
    try {
        var resp = await fetch("/api/v1/settings", {
            method: "PUT",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ llm_provider_mode: mode })
        });
        if (!resp.ok) throw new Error("HTTP " + resp.status);
        updateModeStatus(mode);
        loadProviders();
    } catch(err) {
        alert("Failed to change provider mode: " + err.message);
        loadSettings();
    }
}

function updateModeStatus(mode) {
    var el = document.getElementById("modeStatus");
    if (!el) return;
    if (mode === "byok") {
        var hasActive = providers.some(function(p) { return p.is_active; });
        if (!hasActive) {
            el.innerHTML = "<span class=\"text-warning\"><i data-lucide=\"alert-triangle\" style=\"width:12px;height:12px;display:inline-block;vertical-align:middle;margin-right:2px\"></i>BYOK is selected but no provider is activated. Add an API key below and click the green check to activate it, or chats will fail.</span>";
        } else {
            el.innerHTML = "<span class=\"text-success\"><i data-lucide=\"check-circle\" style=\"width:12px;height:12px;display:inline-block;vertical-align:middle;margin-right:2px\"></i>Using BYOK — chats route through your active provider.</span>";
        }
    } else {
        el.innerHTML = "<span class=\"text-success\"><i data-lucide=\"check-circle\" style=\"width:12px;height:12px;display:inline-block;vertical-align:middle;margin-right:2px\"></i>Using shared Bedrock — AMOS handles the AI access.</span>";
    }
    if (typeof lucide !== "undefined") lucide.createIcons();
}'
    ),
    js_content = REPLACE(
        js_content,
        'function renderProviders() {',
        'function renderProviders() {
    // Re-sync mode status now that providers list is up to date.
    var activeMode = document.querySelector(''input[name="providerMode"]:checked'');
    if (activeMode) updateModeStatus(activeMode.value);
'
    ),
    updated_at = NOW()
WHERE slug = 'system-settings';
