/**
 * AMOS Harness Web UI
 * Production-quality JavaScript for the AMOS conversational AI interface
 */

// App version — bump this to invalidate stale localStorage on deploy
const APP_VERSION = '0.5.0';

// ============================================================================
// State Management
// ============================================================================

const state = {
    currentView: 'chat',
    currentSystemCanvasId: null,
    sessionId: null,
    isStreaming: false,
    darkMode: localStorage.getItem('darkMode') === 'true',
    messages: [],
    currentCanvas: null,
    systemCanvases: [],
    apiBase: window.location.origin,
    currentChatId: null,
    abortController: null,
    pendingAttachments: [], // { id, filename, content_type, size_bytes, url, localPreview }
    planMode: false,
};

// ============================================================================
// Auth-aware fetch wrapper
// ============================================================================

const _originalFetch = window.fetch;
window.fetch = async function(url, options) {
    const response = await _originalFetch(url, options);
    // If any API call returns 401, redirect to platform login
    if (response.status === 401 && typeof url === 'string' && url.includes('/api/')) {
        const platformUrl = window.__AMOS_PLATFORM_URL || 'https://app.amoslabs.com';
        const returnUrl = encodeURIComponent(window.location.href);
        window.location.href = `${platformUrl}/login?redirect=${returnUrl}`;
        return response;
    }
    return response;
};

// ============================================================================
// Initialization
// ============================================================================

document.addEventListener('DOMContentLoaded', function() {
    if (state.darkMode) {
        document.documentElement.classList.add('dark');
        updateDarkModeUI();
    }
    lucide.createIcons();

    // Load system canvases for navigation
    loadSystemNav();

    // Load settings (model selector, provider mode)
    loadSettings();

    // Check for harness updates on load + every 5 min, render banner if available.
    checkForUpdate();
    setInterval(checkForUpdate, 5 * 60 * 1000);

    // Listen for postMessage from canvas iframes
    window.addEventListener('message', handleCanvasMessage);

    // Restore state from previous session
    restoreState();
});

// ============================================================================
// State Persistence
// ============================================================================

function saveState() {
    const toSave = {
        version: APP_VERSION,
        currentView: state.currentView,
        currentSystemCanvasId: state.currentSystemCanvasId,
        sessionId: state.sessionId,
        currentCanvasId: state.currentCanvas ? state.currentCanvas.id : null,
    };
    try {
        localStorage.setItem('amos-state', JSON.stringify(toSave));
    } catch (e) {
        console.warn('Failed to save state:', e);
    }
}

function restoreState() {
    try {
        const saved = localStorage.getItem('amos-state');
        if (!saved) {
            navigate('chat');
            loadRecentSessions();
            document.getElementById('chatInput')?.focus();
            return;
        }

        const data = JSON.parse(saved);

        // Version mismatch — discard stale state
        if (data.version !== APP_VERSION) {
            console.info('App version changed, clearing stale state');
            localStorage.removeItem('amos-state');
            navigate('chat');
            loadRecentSessions();
            document.getElementById('chatInput')?.focus();
            return;
        }

        // Restore session ID and load conversation from server
        if (data.sessionId) {
            state.sessionId = data.sessionId;
            loadSession(data.sessionId);
        }

        // Restore canvas panel — either a system canvas or a user canvas
        if (data.currentSystemCanvasId) {
            state._pendingSystemCanvasId = data.currentSystemCanvasId;
        } else if (data.currentCanvasId) {
            openCanvas(data.currentCanvasId);
        }

        // Always start on the chat view (canvas panel opens alongside it)
        navigate('chat');

        // Load recent sessions for sidebar
        loadRecentSessions();

        lucide.createIcons();
    } catch (e) {
        console.warn('Failed to restore state:', e);
        navigate('chat');
    }

    document.getElementById('chatInput')?.focus();
}

// ============================================================================
// Settings / Model Selector
// ============================================================================

// ============================================================================
// Update banner — polls /api/v1/harness/update-status and renders a
// dismissible banner when a newer release is available. Clicking "Update"
// opens the platform dashboard where the customer hits the Update button.
// Dismissals persist in localStorage keyed by the latest version so the
// banner re-appears for each new release but not on every page load.
// ============================================================================

async function checkForUpdate() {
    try {
        const resp = await fetch('/api/v1/harness/update-status', { credentials: 'include' });
        if (!resp.ok) return;
        const status = await resp.json();
        const banner = document.getElementById('updateBanner');
        if (!status.update_available || !status.latest_version) {
            if (banner) banner.remove();
            return;
        }
        const dismissedKey = 'amos-update-dismissed:' + status.latest_version;
        if (localStorage.getItem(dismissedKey) === 'true') return;
        renderUpdateBanner(status);
    } catch (e) {
        console.warn('Update check failed:', e);
    }
}

function renderUpdateBanner(status) {
    let banner = document.getElementById('updateBanner');
    if (!banner) {
        banner = document.createElement('div');
        banner.id = 'updateBanner';
        banner.className = 'fixed top-0 left-0 right-0 z-40 flex items-center justify-center gap-3 px-4 py-2 bg-purple-600 text-white text-sm shadow-md';
        document.body.insertBefore(banner, document.body.firstChild);
    }
    const updateUrl = status.platform_update_url || 'https://app.amoslabs.com/dashboard';
    const safeVersion = escapeHtml(status.latest_version || 'new version');
    banner.innerHTML =
        '<i data-lucide="arrow-up-circle" class="w-4 h-4"></i>' +
        '<span>A new AMOS Harness release is ready — <strong>' + safeVersion + '</strong></span>' +
        '<a href="' + updateUrl + '" target="_blank" rel="noopener" class="underline font-semibold hover:text-purple-100">Update now</a>' +
        '<button onclick="dismissUpdateBanner(\'' + safeVersion + '\')" class="ml-2 opacity-75 hover:opacity-100" title="Hide until next release"><i data-lucide="x" class="w-4 h-4"></i></button>';
    if (typeof lucide !== 'undefined') lucide.createIcons();
}

function dismissUpdateBanner(version) {
    localStorage.setItem('amos-update-dismissed:' + version, 'true');
    const banner = document.getElementById('updateBanner');
    if (banner) banner.remove();
}

async function loadSettings() {
    try {
        const resp = await fetch('/api/v1/settings', { credentials: 'include' });
        if (!resp.ok) return;
        const settings = await resp.json();

        const selector = document.getElementById('modelSelector');
        const container = document.getElementById('modelSelectorContainer');
        if (!selector || !container) return;

        // Hide model selector if no shared Bedrock and no models to choose
        if (!settings.shared_bedrock_available && settings.llm_provider_mode !== 'shared_bedrock') {
            container.style.display = 'none';
            return;
        }

        // Populate model options
        selector.innerHTML = '';
        for (const model of settings.available_models) {
            const opt = document.createElement('option');
            opt.value = model.id;
            const price = `$${model.input_price_per_mtok}/$${model.output_price_per_mtok} per MTok`;
            opt.textContent = `${model.display_name} (${model.tier})`;
            opt.title = price;
            if (model.id === settings.llm_model) opt.selected = true;
            selector.appendChild(opt);
        }

        // Also store in localStorage for canvas compat
        localStorage.setItem('amos-model', settings.llm_model);
    } catch (e) {
        console.warn('Failed to load settings:', e);
    }
}

async function onModelChange(modelId) {
    try {
        await fetch('/api/v1/settings', {
            method: 'PUT',
            credentials: 'include',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ llm_model: modelId }),
        });
        localStorage.setItem('amos-model', modelId);
    } catch (e) {
        console.warn('Failed to update model:', e);
    }
}

// System Canvas Navigation
// ============================================================================

async function loadSystemNav() {
    const navContainer = document.getElementById('systemNav');

    try {
        const response = await fetch(`${state.apiBase}/api/v1/canvases/system`);
        if (!response.ok) throw new Error('Failed to load system canvases');

        state.systemCanvases = await response.json();

        // Build nav HTML
        let navHtml = state.systemCanvases.map(canvas => {
            const icon = canvas.nav_icon || 'file';
            return `<button onclick="openSystemCanvas('${canvas.id}')" id="nav-sys-${canvas.id}" class="nav-item w-full flex items-center gap-2 px-3 py-2 rounded-lg text-left">
                <i data-lucide="${escapeHtml(icon)}" class="w-4 h-4"></i>
                ${escapeHtml(canvas.name)}
            </button>`;
        }).join('\n');

        navContainer.innerHTML = navHtml;
        lucide.createIcons();

        // If there was a pending system canvas from restore, navigate now
        if (state._pendingSystemCanvasId) {
            openSystemCanvas(state._pendingSystemCanvasId);
            delete state._pendingSystemCanvasId;
        }

    } catch (err) {
        console.error('Failed to load system nav:', err);
        // Fallback: show static nav items
        navContainer.innerHTML = `
            <button onclick="navigate('chat')" class="nav-item active w-full flex items-center gap-2 px-3 py-2 rounded-lg text-left">
                <i data-lucide="message-square" class="w-4 h-4"></i>
                Chat
            </button>
        `;
        lucide.createIcons();
    }
}

function openSystemCanvas(canvasId) {
    state.currentSystemCanvasId = canvasId;

    // Find the canvas data
    const canvas = state.systemCanvases.find(c => c.id === canvasId);
    if (!canvas) {
        console.error('System canvas not found:', canvasId);
        return;
    }

    // Use the same showCanvas() path as regular canvases
    // This gives us chat on left 1/3, canvas on right 2/3
    showCanvas(canvas);

    // Update nav active state AFTER showCanvas (which calls navigate and clears nav)
    document.querySelectorAll('.nav-item').forEach(el => el.classList.remove('active'));
    const navItem = document.getElementById(`nav-sys-${canvasId}`);
    if (navItem) navItem.classList.add('active');

    saveState();
}

// ============================================================================
// postMessage Handler (from canvas iframes)
// ============================================================================

function handleCanvasMessage(event) {
    const data = event.data;
    if (!data || typeof data !== 'object') return;

    switch (data.type) {
        case 'amos-chat':
            // Canvas wants to send a chat message
            if (data.message) {
                navigate('chat');
                sendQuickMessage(data.message);
            }
            break;

        case 'amos-open-canvas':
            // Canvas wants to open a user canvas by ID
            if (data.canvasId) {
                openCanvas(data.canvasId);
            }
            break;

        case 'amos-setting':
            // Canvas changed a setting
            if (data.key === 'model') {
                localStorage.setItem('amos-model', data.value);
            }
            break;

        case 'amos-credential-saved':
            // Secure Input Canvas saved a credential — close canvas and notify agent
            closeCanvas();
            if (data.credential_id && data.service) {
                sendQuickMessage(
                    `I've securely saved my ${data.service} credential. Credential ID: ${data.credential_id}`
                );
            }
            break;

        case 'amos-credential-cancelled':
            // User cancelled the secure input — close canvas
            closeCanvas();
            break;

        default:
            // Ignore unknown message types
            break;
    }
}

// ============================================================================
// Navigation
// ============================================================================

function navigate(view) {
    state.currentView = view;

    // Auto-close sidebar drawer on navigation
    closeSidebar();

    // Hide all views
    document.querySelectorAll('.view').forEach(el => el.classList.add('hidden'));

    // Show target view
    const targetView = document.getElementById(`view-${view}`);
    if (targetView) {
        targetView.classList.remove('hidden');
    }

    // Update nav active state - clear all (system canvas nav is managed by openSystemCanvas)
    document.querySelectorAll('.nav-item').forEach(el => el.classList.remove('active'));

    saveState();
    lucide.createIcons();
}

function toggleDarkMode() {
    state.darkMode = !state.darkMode;
    localStorage.setItem('darkMode', state.darkMode);

    if (state.darkMode) {
        document.documentElement.classList.add('dark');
    } else {
        document.documentElement.classList.remove('dark');
    }

    updateDarkModeUI();
}

function updateDarkModeUI() {
    const icon = document.getElementById('darkModeIcon');
    const label = document.getElementById('darkModeLabel');
    if (state.darkMode) {
        icon.setAttribute('data-lucide', 'sun');
        label.textContent = 'Light Mode';
    } else {
        icon.setAttribute('data-lucide', 'moon');
        label.textContent = 'Dark Mode';
    }
    lucide.createIcons();
}

// ============================================================================
// Chat Functions
// ============================================================================

async function sendMessage() {
    const input = document.getElementById('chatInput');
    const text = input.value.trim();
    if ((!text && state.pendingAttachments.length === 0) || state.isStreaming) return;

    input.value = '';
    autoResize(input);
    state.isStreaming = true;
    state.currentChatId = null;

    // Capture and clear pending attachments
    const attachments = [...state.pendingAttachments];
    state.pendingAttachments = [];
    clearAttachmentPreview();

    // Swap send button to stop button
    const sendBtn = document.getElementById('sendBtn');
    sendBtn.innerHTML = '<i data-lucide="square" class="w-4 h-4"></i>';
    sendBtn.onclick = stopChat;
    sendBtn.disabled = false;
    sendBtn.title = 'Stop';
    sendBtn.classList.remove('bg-amos-600', 'hover:bg-amos-700');
    sendBtn.classList.add('bg-red-500', 'hover:bg-red-600');
    lucide.createIcons();

    // Create an AbortController so we can abort the fetch
    state.abortController = new AbortController();

    // Hide welcome message
    const welcomeMsg = document.getElementById('welcomeMessage');
    if (welcomeMsg) {
        welcomeMsg.remove();
    }

    // Make sure we're on the chat view
    if (state.currentView !== 'chat') {
        navigate('chat');
    }

    // Add user message (with inline attachment previews)
    const userEl = appendMessage('user', text);
    if (attachments.length > 0) {
        const previewHtml = attachments.map(att => {
            if (att.content_type && att.content_type.startsWith('image/') && att.localPreview) {
                return `<img src="${att.localPreview}" alt="${escapeHtml(att.filename)}" class="inline-block max-h-32 rounded-lg mt-1 border border-gray-200 dark:border-gray-600">`;
            }
            return `<div class="inline-flex items-center gap-1 px-2 py-1 mt-1 rounded bg-gray-100 dark:bg-gray-700 text-xs"><i data-lucide="file" class="w-3 h-3"></i>${escapeHtml(att.filename)}</div>`;
        }).join(' ');
        const content = userEl.querySelector('.message-content');
        if (content) content.innerHTML += previewHtml;
        lucide.createIcons();
    }

    // Create assistant message placeholder with thinking indicator
    const assistantEl = appendMessage('assistant', '');
    const thinkingEl = document.createElement('div');
    thinkingEl.className = 'thinking-indicator';
    thinkingEl.innerHTML = '<div class="thinking-dots"><span></span><span></span><span></span></div>';
    assistantEl.querySelector('.message-content').appendChild(thinkingEl);

    try {
        const requestBody = {
            message: text || '(see attached files)',
            session_id: state.sessionId,
        };
        if (state.planMode) {
            requestBody.plan_mode = true;
        }
        if (attachments.length > 0) {
            requestBody.attachments = attachments.map(a => a.id);
        }

        const response = await fetch(`${state.apiBase}/api/v1/agent/chat`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(requestBody),
            signal: state.abortController.signal,
        });

        if (!response.ok) {
            throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        let fullText = '';
        let hasError = false;
        let currentToolIndicator = null;

        while (true) {
            const { done, value } = await reader.read();
            if (done) break;

            buffer += decoder.decode(value, { stream: true });
            const lines = buffer.split('\n');
            buffer = lines.pop() || '';

            for (const line of lines) {
                if (!line.trim()) continue;

                // Handle named SSE events (event: chat_id)
                if (line.startsWith('event:')) {
                    // Store event type for next data line
                    state._pendingEventType = line.slice(6).trim();
                    continue;
                }

                if (line.startsWith('data:')) {
                    const jsonStr = line.slice(5).trim();
                    if (!jsonStr) continue;

                    try {
                        const data = JSON.parse(jsonStr);

                        // Handle chat_meta event (sent as named SSE event)
                        if (state._pendingEventType === 'chat_meta') {
                            if (data.chat_id) state.currentChatId = data.chat_id;
                            if (data.session_id) {
                                state.sessionId = data.session_id;
                                saveState();
                            }
                            console.log('Chat meta:', data);
                            state._pendingEventType = null;
                            continue;
                        }
                        state._pendingEventType = null;

                        // Handle turn_start event
                        if (data.type === 'turn_start') {
                            console.log('Turn started:', data.iteration, 'Model:', data.model);
                        }
                        // Handle message_start event
                        else if (data.type === 'message_start') {
                            console.log('Message started, role:', data.role);
                        }
                        // Handle message_delta event - append content
                        else if (data.type === 'message_delta' && data.content) {
                            // Remove thinking indicator on first content
                            const thinking = assistantEl.querySelector('.thinking-indicator');
                            if (thinking) thinking.remove();

                            // Remove tool indicator if present
                            if (currentToolIndicator) {
                                currentToolIndicator.remove();
                                currentToolIndicator = null;
                            }

                            fullText += data.content;
                            const content = assistantEl.querySelector('.message-content');
                            if (content) {
                                content.innerHTML = formatMarkdown(fullText);
                            }
                        }
                        // Handle message_end event
                        else if (data.type === 'message_end') {
                            console.log('Message ended');
                        }
                        // Handle tool_start event - show tool indicator
                        else if (data.type === 'tool_start') {
                            // Remove thinking indicator when tools start
                            const thinking = assistantEl.querySelector('.thinking-indicator');
                            if (thinking) thinking.remove();
                            currentToolIndicator = showToolIndicator(assistantEl, data.tool_name, data.tool_input, data.input_summary);
                        }
                        // Handle tool_end event - remove tool indicator, show completion
                        else if (data.type === 'tool_end') {
                            if (currentToolIndicator) {
                                currentToolIndicator.remove();
                                currentToolIndicator = null;
                            }
                            console.log('Tool completed:', data.tool_name, 'Duration:', data.duration_ms, 'ms');
                            // Show completed step in activity log
                            appendToolActivity(assistantEl, data.tool_name, data.duration_ms, data.is_error, data.result_summary);

                            // Check for canvas actions in tool result metadata
                            const metadata = data.metadata;
                            if (metadata && metadata.__confirmation_required) {
                                showCommandConfirmation(assistantEl, metadata.__confirmation_required);
                            } else if (metadata && metadata.__canvas_action === 'secure_input') {
                                openSecureInputCanvas(metadata);
                            } else if (metadata && metadata.__canvas_action === 'preview_site') {
                                openSitePreview(metadata.url, metadata.site_slug);
                            } else if (metadata && metadata.__canvas_action === 'preview_app') {
                                openAppPreview(metadata.canvas_id, metadata.app_name);
                            }
                        }
                        // Handle turn_end event
                        else if (data.type === 'turn_end') {
                            console.log('Turn ended, tokens used:', data.tokens_used);
                        }
                        // Handle agent_end event - stream complete
                        else if (data.type === 'agent_end') {
                            console.log('Agent completed:', data.reason, 'Total iterations:', data.total_iterations, 'Total tokens:', data.total_tokens);
                            // Remove any remaining tool indicator
                            if (currentToolIndicator) {
                                currentToolIndicator.remove();
                                currentToolIndicator = null;
                            }
                        }
                        // Handle tool_input_delta event - streaming tool input
                        else if (data.type === 'tool_input_delta') {
                            // Update tool indicator with streaming input preview
                            if (currentToolIndicator) {
                                const preview = currentToolIndicator.querySelector('.tool-input-preview');
                                if (preview) {
                                    preview.textContent += data.partial_input;
                                }
                            }
                        }
                        // Handle compacted event - conversation was auto-compacted
                        else if (data.type === 'compacted') {
                            console.log('Conversation compacted:', data.removed_messages, 'messages removed,', data.estimated_tokens, 'tokens remaining');
                            appendToolActivity(assistantEl, 'auto-compact', 0, false, `Compacted ${data.removed_messages} messages to stay within context limit`);
                        }
                        // Handle hook_denied event - a hook blocked tool execution
                        else if (data.type === 'hook_denied') {
                            console.warn('Hook denied:', data.tool_name, data.hook_message);
                            appendToolActivity(assistantEl, data.tool_name, 0, true, `Blocked by hook: ${data.hook_message}`);
                        }
                        // Handle model_escalation event
                        else if (data.type === 'model_escalation') {
                            console.log('Model escalated from', data.from_model, 'to', data.to_model, 'Reason:', data.reason);
                        }
                        // Handle error event
                        else if (data.type === 'error') {
                            console.error('Agent error:', data.message);
                            const content = assistantEl.querySelector('.message-content');
                            if (content) {
                                // Show user-friendly error for rate limits
                                let userMsg = data.message || 'Unknown error';
                                if (userMsg.includes('429') || userMsg.includes('rate_limit') || userMsg.includes('Too Many Requests')) {
                                    userMsg = 'Your API provider rate limit was exceeded. Try starting a new conversation to reduce context size, or wait a minute before retrying.';
                                }
                                content.innerHTML = `<div class="error-message"><strong>Error:</strong> ${escapeHtml(userMsg)}</div>`;
                            }
                            hasError = true;
                        }
                    } catch (parseError) {
                        // Ignore AbortError from stop
                        if (parseError.name === 'AbortError') throw parseError;
                        console.error('Failed to parse SSE data:', parseError, 'Line:', jsonStr);
                    }
                }
            }
        }

        // If no text was received and no error occurred, show a default message
        if (!fullText && !hasError) {
            const content = assistantEl.querySelector('.message-content');
            if (content && !content.innerHTML.trim()) {
                content.innerHTML = '<p class="text-gray-500">I completed your request.</p>';
            }
        }

    } catch (err) {
        if (err.name === 'AbortError') {
            // User pressed stop — show a subtle indicator
            const content = assistantEl.querySelector('.message-content');
            if (content) {
                const stopNote = document.createElement('p');
                stopNote.className = 'text-xs text-gray-400 italic mt-2';
                stopNote.textContent = 'Stopped by user';
                content.appendChild(stopNote);
            }
        } else {
            console.error('Chat error:', err);
            const content = assistantEl.querySelector('.message-content');
            if (content) {
                content.innerHTML = `<div class="error-message"><strong>Error:</strong> ${escapeHtml(err.message)}</div>`;
            }
        }
    } finally {
        // Remove thinking indicator if still present
        const thinking = assistantEl.querySelector('.thinking-indicator');
        if (thinking) thinking.remove();
        state.isStreaming = false;
        state.currentChatId = null;
        state.abortController = null;

        // Restore send button
        sendBtn.innerHTML = '<i data-lucide="send" class="w-4 h-4"></i>';
        sendBtn.onclick = sendMessage;
        sendBtn.disabled = false;
        sendBtn.title = '';
        sendBtn.classList.remove('bg-red-500', 'hover:bg-red-600');
        sendBtn.classList.add('bg-amos-600', 'hover:bg-amos-700');

        saveState();
        lucide.createIcons();

        // Refresh sidebar with latest sessions
        loadRecentSessions();
    }
}

async function stopChat() {
    if (!state.isStreaming) return;

    // 1. Tell the server to cancel the agent loop
    if (state.currentChatId) {
        try {
            await fetch(`${state.apiBase}/api/v1/agent/chat/cancel`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ chat_id: state.currentChatId }),
            });
        } catch (e) {
            console.warn('Failed to send cancel request:', e);
        }
    }

    // 2. Abort the client-side fetch to stop reading the stream
    if (state.abortController) {
        state.abortController.abort();
    }
}

// ============================================================================
// Session Persistence
// ============================================================================

/**
 * Load a session from the server and rebuild the chat UI.
 * Called on page refresh to restore the conversation.
 */
async function loadSession(sessionId) {
    try {
        const response = await fetch(`${state.apiBase}/api/v1/agent/sessions/${sessionId}`);
        if (!response.ok) {
            console.warn('Session not found on server, starting fresh');
            state.sessionId = null;
            saveState();
            return;
        }

        const data = await response.json();
        const messages = data.messages || [];

        if (messages.length === 0) return;

        // Hide welcome message
        const welcomeMsg = document.getElementById('welcomeMessage');
        if (welcomeMsg) welcomeMsg.remove();

        // Clear existing messages
        const container = document.getElementById('chatMessages');
        container.innerHTML = '';

        // Rebuild the conversation UI
        for (const msg of messages) {
            const role = msg.role;
            if (role === 'user') {
                const text = extractTextFromContent(msg.content);
                const images = extractImagesFromContent(msg.content);
                if (text || images.length > 0) {
                    const el = appendMessage('user', text);
                    // Show image attachments inline
                    if (images.length > 0) {
                        const content = el.querySelector('.message-content');
                        if (content) {
                            const imgHtml = images.map(img =>
                                `<img src="data:${escapeHtml(img.media_type)};base64,${img.data}" alt="attached image" class="inline-block max-h-32 rounded-lg mt-1 border border-gray-200 dark:border-gray-600">`
                            ).join(' ');
                            content.innerHTML += imgHtml;
                        }
                    }
                }
            } else if (role === 'assistant') {
                const text = extractTextFromContent(msg.content);
                if (text) appendMessage('assistant', text);
            }
            // Skip tool result messages — they're internal
        }

        container.scrollTop = container.scrollHeight;
    } catch (e) {
        console.warn('Failed to load session:', e);
    }
}

/**
 * Extract readable text from an array of content blocks.
 */
function extractTextFromContent(content) {
    if (!content || !Array.isArray(content)) return '';
    return content
        .filter(block => block.type === 'text' && block.text)
        .map(block => block.text)
        .join('\n');
}

/**
 * Extract image blocks from an array of content blocks.
 * Returns array of { media_type, data } objects (base64 images).
 */
function extractImagesFromContent(content) {
    if (!content || !Array.isArray(content)) return [];
    return content
        .filter(block => block.type === 'image' && block.source)
        .map(block => ({
            media_type: block.source.media_type || 'image/png',
            data: block.source.data || '',
        }));
}

/**
 * Load recent sessions for the sidebar.
 */
async function loadRecentSessions() {
    const container = document.getElementById('recentChats');
    if (!container) return;

    try {
        const response = await fetch(`${state.apiBase}/api/v1/agent/sessions?limit=15`);
        if (!response.ok) {
            container.innerHTML = '';
            return;
        }

        const data = await response.json();
        const sessions = data.sessions || [];

        if (sessions.length === 0) {
            container.innerHTML = '<p class="text-xs text-gray-400 px-3 py-2">No conversations yet</p>';
            return;
        }

        container.innerHTML = sessions.map(session => {
            const title = session.title || 'Untitled conversation';
            const truncatedTitle = title.length > 40 ? title.substring(0, 40) + '...' : title;
            const isActive = state.sessionId === session.id;
            const activeClass = isActive ? 'bg-gray-100 dark:bg-gray-800' : '';

            return `<button onclick="resumeSession('${session.id}')"
                class="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-left text-sm hover:bg-gray-50 dark:hover:bg-gray-800 ${activeClass} truncate"
                title="${escapeHtml(title)}">
                <i data-lucide="message-square" class="w-3 h-3 flex-shrink-0 text-gray-400"></i>
                <span class="truncate">${escapeHtml(truncatedTitle)}</span>
            </button>`;
        }).join('\n');

        lucide.createIcons();
    } catch (e) {
        console.warn('Failed to load recent sessions:', e);
    }
}

/**
 * Resume a previous session from the sidebar.
 */
function resumeSession(sessionId) {
    state.sessionId = sessionId;
    saveState();
    loadSession(sessionId);
    closeSidebar();
    navigate('chat');
}

function sendQuickMessage(text) {
    const input = document.getElementById('chatInput');
    input.value = text;
    sendMessage();
}

function newChat() {
    state.sessionId = null;
    state.messages = [];
    const container = document.getElementById('chatMessages');
    container.innerHTML = `
        <div id="welcomeMessage" class="flex flex-col items-center justify-center h-full text-center">
            <div class="w-16 h-16 rounded-2xl bg-amos-100 dark:bg-amos-900 flex items-center justify-center mb-4">
                <i data-lucide="sparkles" class="w-8 h-8 text-amos-600"></i>
            </div>
            <h2 class="text-2xl font-semibold mb-2">Welcome to AMOS</h2>
            <p class="text-gray-500 dark:text-gray-400 max-w-md">Your AI-powered business operating system. Tell me what you need — I'll figure out the best way to make it happen.</p>
            <div class="mt-6 grid grid-cols-2 gap-3 max-w-lg">
                <button onclick="sendQuickMessage('How is my business doing?')" class="quick-action p-3 rounded-xl border border-gray-200 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 text-left text-sm">
                    <i data-lucide="bar-chart-3" class="w-5 h-5 text-amos-500 mb-1"></i>
                    <p class="font-medium">Business Overview</p>
                    <p class="text-gray-500 text-xs">See where things stand</p>
                </button>
                <button onclick="sendQuickMessage('Help me keep track of my customers and deals')" class="quick-action p-3 rounded-xl border border-gray-200 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 text-left text-sm">
                    <i data-lucide="users" class="w-5 h-5 text-amos-500 mb-1"></i>
                    <p class="font-medium">Customer Tracking</p>
                    <p class="text-gray-500 text-xs">Manage relationships</p>
                </button>
                <button onclick="sendQuickMessage('I need help automating part of my workflow')" class="quick-action p-3 rounded-xl border border-gray-200 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 text-left text-sm">
                    <i data-lucide="workflow" class="w-5 h-5 text-amos-500 mb-1"></i>
                    <p class="font-medium">Automate Something</p>
                    <p class="text-gray-500 text-xs">Save time on repetitive work</p>
                </button>
                <button onclick="sendQuickMessage('What can you do?')" class="quick-action p-3 rounded-xl border border-gray-200 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 text-left text-sm">
                    <i data-lucide="sparkles" class="w-5 h-5 text-amos-500 mb-1"></i>
                    <p class="font-medium">Explore Capabilities</p>
                    <p class="text-gray-500 text-xs">See what AMOS can do</p>
                </button>
            </div>
        </div>
    `;
    lucide.createIcons();
    closeCanvas();
    navigate('chat');
    saveState();
    loadRecentSessions();
}

function appendMessage(role, content) {
    const container = document.getElementById('chatMessages');
    const div = document.createElement('div');
    div.className = `message-row message-${role} p-4`;

    const iconName = role === 'user' ? 'user' : 'sparkles';
    div.innerHTML = `<div class="message-icon"><i data-lucide="${iconName}" class="w-4 h-4"></i></div><div class="message-body"><div class="tool-activity-log" style="display:none"><div class="activity-items"></div></div><div class="message-content">${content ? formatMarkdown(content) : ''}</div></div>`;
    container.appendChild(div);
    container.scrollTop = container.scrollHeight;
    lucide.createIcons({attrs: {}, nameAttr: 'data-lucide'});
    return div;
}

function showToolIndicator(messageEl, toolName, toolInput, inputSummary) {
    const content = messageEl.querySelector('.message-content');
    const indicator = document.createElement('div');
    indicator.className = 'tool-indicator';

    const toolDisplayNames = {
        'create_canvas': 'Creating canvas',
        'update_canvas': 'Updating canvas',
        'manage_bot': 'Managing bot',
        'query_database': 'Querying database',
        'call_api': 'Calling API',
        'run_code': 'Running code',
        'collect_credential': 'Preparing secure input',
        'list_vault_credentials': 'Checking stored credentials',
        'define_collection': 'Defining collection',
        'create_record': 'Creating record',
        'update_record': 'Updating record',
        'delete_record': 'Deleting record',
        'query_records': 'Querying records',
        'create_site': 'Creating site',
        'create_page': 'Building page',
        'update_page': 'Updating page',
        'publish_site': 'Publishing site',
        'create_automation': 'Creating automation',
        'ingest_document': 'Ingesting document',
        'search_knowledge': 'Searching knowledge base',
        'web_search': 'Searching the web',
        'get_workspace_summary': 'Loading workspace',
        'create_app': 'Building app',
        'update_app_view': 'Updating app view',
        'list_available_specialists': 'Checking available specialists',
        'activate_specialist': 'Setting up specialist',
        'deactivate_specialist': 'Shutting down specialist',
        'list_harnesses': 'Checking active specialists',
        'delegate_to_harness': 'Working with specialist',
        'submit_task_to_harness': 'Sending task to specialist',
        'get_harness_status': 'Checking specialist status',
        'broadcast_to_harnesses': 'Coordinating specialists',
    };

    // Use input_summary if provided, otherwise fall back to display name map
    const displayName = inputSummary || toolDisplayNames[toolName] || `Using ${toolName}`;

    indicator.innerHTML = `
        <div class="spinner"></div>
        <span>${escapeHtml(displayName)}...</span>
    `;

    content.appendChild(indicator);
    return indicator;
}

/**
 * Show a completed tool step in the activity log area within the message.
 * The activity log is a sibling of .message-content so it persists across
 * content re-renders from message_delta events.
 */
function appendToolActivity(messageEl, toolName, durationMs, isError, resultSummary) {
    const activityLog = messageEl.querySelector('.tool-activity-log');
    if (!activityLog) return;

    // Hide errors from internal context-gathering tools — these are not
    // user-requested and their failure shouldn't clutter the conversation.
    const internalTools = ['get_workspace_summary', 'get_memory_context'];
    if (isError && internalTools.includes(toolName)) {
        console.warn(`Internal tool ${toolName} failed (suppressed):`, resultSummary);
        return;
    }

    activityLog.style.display = '';
    const items = activityLog.querySelector('.activity-items');
    const item = document.createElement('div');
    item.className = 'activity-item' + (isError ? ' activity-error' : '');

    const icon = isError ? '✗' : '✓';
    const summary = resultSummary || formatToolName(toolName);
    const duration = durationMs > 0 ? `${(durationMs / 1000).toFixed(1)}s` : '';

    item.innerHTML = `<span class="activity-icon">${icon}</span> <span class="activity-text">${escapeHtml(summary)}</span>${duration ? ` <span class="activity-duration">${duration}</span>` : ''}`;
    items.appendChild(item);

    // Auto-scroll to keep activity visible
    const container = document.getElementById('chatMessages');
    if (container) container.scrollTop = container.scrollHeight;
}

function formatToolName(name) {
    return name.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
}

function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

// ============================================================================
// Destructive Command Confirmation
// ============================================================================

/**
 * Show an inline approve/deny prompt for a destructive bash command.
 * Called when a tool_end event contains __confirmation_required metadata.
 *
 * @param {HTMLElement} messageEl - The assistant message element
 * @param {Object} confirmation - { token, command, warning }
 */
function showCommandConfirmation(messageEl, confirmation) {
    const activityLog = messageEl.querySelector('.tool-activity-log');
    if (!activityLog) return;
    activityLog.style.display = '';

    const items = activityLog.querySelector('.activity-items');
    const confirmEl = document.createElement('div');
    confirmEl.className = 'confirmation-prompt';
    confirmEl.innerHTML = `
        <div class="flex flex-col gap-2 p-3 my-2 rounded-lg border border-amber-300 dark:border-amber-600 bg-amber-50 dark:bg-amber-950/30">
            <div class="flex items-center gap-2 text-amber-700 dark:text-amber-400 text-sm font-medium">
                <i data-lucide="shield-alert" class="w-4 h-4"></i>
                <span>Confirmation Required</span>
            </div>
            <div class="text-xs text-gray-600 dark:text-gray-400">${escapeHtml(confirmation.warning)}</div>
            <code class="block text-xs p-2 rounded bg-gray-100 dark:bg-gray-800 text-gray-800 dark:text-gray-200 overflow-x-auto">${escapeHtml(confirmation.command)}</code>
            <div class="flex gap-2 mt-1">
                <button class="confirm-approve-btn inline-flex items-center gap-1 px-3 py-1.5 text-xs font-medium rounded-md bg-green-600 hover:bg-green-700 text-white transition-colors">
                    <i data-lucide="check" class="w-3 h-3"></i> Approve
                </button>
                <button class="confirm-deny-btn inline-flex items-center gap-1 px-3 py-1.5 text-xs font-medium rounded-md bg-red-500 hover:bg-red-600 text-white transition-colors">
                    <i data-lucide="x" class="w-3 h-3"></i> Deny
                </button>
            </div>
        </div>
    `;
    items.appendChild(confirmEl);
    lucide.createIcons();

    const approveBtn = confirmEl.querySelector('.confirm-approve-btn');
    const denyBtn = confirmEl.querySelector('.confirm-deny-btn');

    approveBtn.addEventListener('click', () => handleConfirmation(confirmEl, confirmation, true));
    denyBtn.addEventListener('click', () => handleConfirmation(confirmEl, confirmation, false));

    // Auto-scroll
    const container = document.getElementById('chatMessages');
    if (container) container.scrollTop = container.scrollHeight;
}

/**
 * Handle the user's confirmation decision (approve or deny).
 * Calls the confirm API endpoint and injects the result into chat.
 */
async function handleConfirmation(confirmEl, confirmation, approved) {
    // Disable buttons immediately
    const buttons = confirmEl.querySelectorAll('button');
    buttons.forEach(btn => { btn.disabled = true; btn.classList.add('opacity-50', 'cursor-not-allowed'); });

    // Replace the prompt with a status indicator
    const statusDiv = confirmEl.querySelector('.flex.flex-col');
    const statusText = approved ? 'Executing...' : 'Denied';
    const statusColor = approved ? 'text-green-600 dark:text-green-400' : 'text-red-500 dark:text-red-400';
    const statusIcon = approved ? 'loader' : 'x-circle';

    // Update the button area with status
    const buttonArea = confirmEl.querySelector('.flex.gap-2.mt-1');
    buttonArea.innerHTML = `<span class="inline-flex items-center gap-1 text-xs ${statusColor}"><i data-lucide="${statusIcon}" class="w-3 h-3 ${approved ? 'animate-spin' : ''}"></i> ${statusText}</span>`;
    lucide.createIcons();

    try {
        const response = await fetch(`${state.apiBase}/api/v1/tools/confirm`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ token: confirmation.token, approved }),
        });

        const result = await response.json();

        if (!approved) {
            // User denied — update UI and send denial to agent
            buttonArea.innerHTML = `<span class="inline-flex items-center gap-1 text-xs text-red-500 dark:text-red-400"><i data-lucide="x-circle" class="w-3 h-3"></i> Command denied by user</span>`;
            lucide.createIcons();
            sendFollowUpMessage(`I denied the command: \`${confirmation.command}\`. Do not execute it.`);
            return;
        }

        if (result.status === 'executed') {
            // Show execution result
            const success = result.success;
            const icon = success ? 'check-circle' : 'alert-circle';
            const color = success ? 'text-green-600 dark:text-green-400' : 'text-red-500 dark:text-red-400';
            buttonArea.innerHTML = `<span class="inline-flex items-center gap-1 text-xs ${color}"><i data-lucide="${icon}" class="w-3 h-3"></i> ${success ? 'Executed successfully' : 'Command failed'} (exit ${result.exit_code})</span>`;
            lucide.createIcons();

            // Build result text for the agent
            let resultText = `Command executed: \`${confirmation.command}\`\n`;
            if (result.stdout) resultText += `\nstdout:\n\`\`\`\n${result.stdout}\n\`\`\``;
            if (result.stderr) resultText += `\nstderr:\n\`\`\`\n${result.stderr}\n\`\`\``;
            resultText += `\nexit code: ${result.exit_code}`;

            // Show output in the UI if there is any
            if (result.stdout || result.stderr) {
                const outputEl = document.createElement('div');
                outputEl.className = 'text-xs mt-2 p-2 rounded bg-gray-100 dark:bg-gray-800 max-h-40 overflow-auto';
                let outputHtml = '';
                if (result.stdout) outputHtml += `<pre class="text-gray-700 dark:text-gray-300 whitespace-pre-wrap">${escapeHtml(result.stdout)}</pre>`;
                if (result.stderr) outputHtml += `<pre class="text-red-600 dark:text-red-400 whitespace-pre-wrap">${escapeHtml(result.stderr)}</pre>`;
                outputEl.innerHTML = outputHtml;
                statusDiv.appendChild(outputEl);
            }

            // Send result to agent so it can continue reasoning
            sendFollowUpMessage(resultText);
        } else if (result.status === 'error' || result.status === 'timeout') {
            buttonArea.innerHTML = `<span class="inline-flex items-center gap-1 text-xs text-red-500 dark:text-red-400"><i data-lucide="alert-circle" class="w-3 h-3"></i> ${escapeHtml(result.message)}</span>`;
            lucide.createIcons();
            sendFollowUpMessage(`Command failed: ${result.message}`);
        }
    } catch (err) {
        buttonArea.innerHTML = `<span class="inline-flex items-center gap-1 text-xs text-red-500 dark:text-red-400"><i data-lucide="alert-circle" class="w-3 h-3"></i> Failed to confirm: ${escapeHtml(err.message)}</span>`;
        lucide.createIcons();
    }

    // Auto-scroll
    const container = document.getElementById('chatMessages');
    if (container) container.scrollTop = container.scrollHeight;
}

/**
 * Send a follow-up message to the agent with command execution results.
 * This is a lightweight message send that doesn't create a new user bubble —
 * it just feeds the result back into the conversation so the agent can continue.
 */
function sendFollowUpMessage(text) {
    // Only send if we have a session
    if (!state.sessionId) return;

    // Fire and forget — the agent will pick up the result in its next turn
    fetch(`${state.apiBase}/api/v1/agent/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
            message: text,
            session_id: state.sessionId,
        }),
    }).catch(err => {
        console.warn('Failed to send follow-up message:', err);
    });
}

// ============================================================================
// Secure Input Canvas (Credential Collection)
// ============================================================================

/**
 * Open the Secure Input Canvas in the canvas panel.
 * Called when a tool_end event contains __canvas_action: "secure_input".
 * The canvas shows a masked input field and POSTs directly to /api/v1/credentials.
 */
function openSecureInputCanvas(metadata) {
    const service = metadata.service || 'unknown';
    const label = metadata.label || 'Credential';
    const credentialType = metadata.credential_type || 'api_key';
    const instructions = metadata.instructions || '';
    const placeholder = metadata.placeholder || '';

    const typeLabel = credentialType.replace(/_/g, ' ');

    const html = `
<div class="container py-4" style="max-width: 520px; margin: 0 auto;">
    <div class="text-center mb-4">
        <div class="d-inline-flex align-items-center justify-content-center rounded-circle bg-warning bg-opacity-10 mb-3" style="width:64px;height:64px;">
            <i data-lucide="shield-check" class="text-warning" style="width:32px;height:32px;"></i>
        </div>
        <h4 class="mb-1">Secure Credential Input</h4>
        <p class="text-muted small mb-0">Your ${escapeAttr(typeLabel)} for <strong>${escapeAttr(service)}</strong> will be encrypted and stored securely. It will never appear in the chat.</p>
    </div>

    ${instructions ? `<div class="alert alert-info small"><i data-lucide="info" class="me-1" style="width:14px;height:14px;"></i> ${escapeAttr(instructions)}</div>` : ''}

    <form id="secureInputForm" autocomplete="off">
        <label for="secretValue" class="form-label fw-semibold">${escapeAttr(label)}</label>
        <div class="input-group mb-3">
            <input type="password" class="form-control form-control-lg" id="secretValue"
                   placeholder="${escapeAttr(placeholder)}" autocomplete="off" spellcheck="false"
                   style="font-family: monospace; letter-spacing: 0.05em;">
            <button class="btn btn-outline-secondary" type="button" id="toggleVisibility" title="Show/hide">
                <i data-lucide="eye" id="visIcon" style="width:18px;height:18px;"></i>
            </button>
        </div>

        <div id="errorAlert" class="alert alert-danger small d-none"></div>
        <div id="successAlert" class="alert alert-success small d-none"></div>

        <div class="d-grid gap-2">
            <button type="submit" class="btn btn-warning btn-lg" id="submitBtn">
                <i data-lucide="lock" class="me-1" style="width:18px;height:18px;"></i>
                Encrypt & Save
            </button>
            <button type="button" class="btn btn-outline-secondary" id="cancelBtn">Cancel</button>
        </div>
    </form>
</div>`;

    const css = `
body { background: #f8f9fa; }
.form-control:focus { border-color: #ffc107; box-shadow: 0 0 0 0.25rem rgba(255,193,7,.25); }
`;

    const js = `
(function() {
    const form = document.getElementById('secureInputForm');
    const input = document.getElementById('secretValue');
    const toggleBtn = document.getElementById('toggleVisibility');
    const visIcon = document.getElementById('visIcon');
    const submitBtn = document.getElementById('submitBtn');
    const cancelBtn = document.getElementById('cancelBtn');
    const errorAlert = document.getElementById('errorAlert');
    const successAlert = document.getElementById('successAlert');

    // Toggle password visibility
    toggleBtn.addEventListener('click', function() {
        if (input.type === 'password') {
            input.type = 'text';
            visIcon.setAttribute('data-lucide', 'eye-off');
        } else {
            input.type = 'password';
            visIcon.setAttribute('data-lucide', 'eye');
        }
        if (typeof lucide !== 'undefined') lucide.createIcons();
    });

    // Cancel
    cancelBtn.addEventListener('click', function() {
        window.parent.postMessage({ type: 'amos-credential-cancelled' }, '*');
    });

    // Submit
    form.addEventListener('submit', async function(e) {
        e.preventDefault();
        errorAlert.classList.add('d-none');
        successAlert.classList.add('d-none');

        const value = input.value.trim();
        if (!value) {
            errorAlert.textContent = 'Please enter a value.';
            errorAlert.classList.remove('d-none');
            return;
        }

        submitBtn.disabled = true;
        submitBtn.innerHTML = '<span class="spinner-border spinner-border-sm me-1"></span> Encrypting...';

        try {
            const response = await fetch('/api/v1/credentials', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    service: ${JSON.stringify(service)},
                    label: ${JSON.stringify(label)},
                    credential_type: ${JSON.stringify(credentialType)},
                    secret_value: value,
                }),
            });

            if (!response.ok) {
                const errData = await response.json().catch(() => ({}));
                throw new Error(errData.error || 'Failed to save credential (HTTP ' + response.status + ')');
            }

            const result = await response.json();

            // Clear the input immediately
            input.value = '';

            successAlert.textContent = 'Credential saved securely.';
            successAlert.classList.remove('d-none');

            // Notify parent after brief delay so user sees success
            setTimeout(function() {
                window.parent.postMessage({
                    type: 'amos-credential-saved',
                    credential_id: result.credential_id,
                    service: ${JSON.stringify(service)},
                }, '*');
            }, 800);

        } catch (err) {
            errorAlert.textContent = err.message;
            errorAlert.classList.remove('d-none');
            submitBtn.disabled = false;
            submitBtn.innerHTML = '<i data-lucide="lock" class="me-1" style="width:18px;height:18px;"></i> Encrypt & Save';
            if (typeof lucide !== 'undefined') lucide.createIcons();
        }
    });

    // Focus the input on load
    input.focus();
})();
`;

    // Use the existing canvas infrastructure to display
    showCanvas({
        name: 'Secure Input - ' + label,
        html_content: html,
        css_content: css,
        javascript: js,
    });
}

/**
 * Escape a string for use in HTML attributes (used by secure input canvas).
 */
function escapeAttr(str) {
    if (!str) return '';
    return str.replace(/&/g, '&amp;').replace(/"/g, '&quot;')
              .replace(/'/g, '&#39;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// ============================================================================
// Markdown Formatting
// ============================================================================

function formatMarkdown(text) {
    if (!text) return '';

    let html = escapeHtml(text);

    // Code blocks (must be before inline code)
    html = html.replace(/```(\w+)?\n([\s\S]*?)```/g, (match, lang, code) => {
        return `<pre><code>${code.trim()}</code></pre>`;
    });

    // Inline code
    html = html.replace(/`([^`]+)`/g, '<code>$1</code>');

    // Headers
    html = html.replace(/^### (.*$)/gm, '<h3>$1</h3>');
    html = html.replace(/^## (.*$)/gm, '<h2>$1</h2>');
    html = html.replace(/^# (.*$)/gm, '<h1>$1</h1>');

    // Bold
    html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    html = html.replace(/__([^_]+)__/g, '<strong>$1</strong>');

    // Italic
    html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>');
    html = html.replace(/_([^_]+)_/g, '<em>$1</em>');

    // Links
    html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" class="text-amos-600 hover:underline">$1</a>');

    // Blockquotes
    html = html.replace(/^&gt; (.*$)/gm, '<blockquote>$1</blockquote>');

    // Lists (unordered)
    html = html.replace(/^\* (.*$)/gm, '<li>$1</li>');
    html = html.replace(/^- (.*$)/gm, '<li>$1</li>');

    // Lists (ordered)
    html = html.replace(/^\d+\. (.*$)/gm, '<li>$1</li>');

    // Wrap consecutive <li> tags in <ul> or <ol>
    html = html.replace(/(<li>.*<\/li>\n?)+/g, (match) => {
        return '<ul>' + match + '</ul>';
    });

    // Line breaks and paragraphs
    html = html.replace(/\n\n/g, '</p><p>');
    html = html.replace(/\n/g, '<br>');

    // Wrap in paragraph if not already wrapped
    if (!html.startsWith('<')) {
        html = '<p>' + html + '</p>';
    }

    return html;
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// ============================================================================
// Canvas Functions
// ============================================================================

/**
 * Build a full HTML document for canvas iframe/popup.
 * Includes Bootstrap 5 CSS, Lucide icons, and the canvas's own CSS/JS.
 */
function buildCanvasDocument(title, html, css, js) {
    // Base href ensures relative URLs (e.g. /api/v1/canvases) resolve against the server
    // origin, since the iframe loads from a blob: URL which has no inherent base.
    const baseHref = window.location.origin;

    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <base href="${baseHref}/">
    <title>${title}</title>
    <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css" rel="stylesheet">
    <script src="https://unpkg.com/lucide@latest"></script>
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4"></script>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; }
        ${css}
    </style>
</head>
<body>
    ${html}
    <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/js/bootstrap.bundle.min.js"><\/script>
    <script src="/js/amos-components.js"><\/script>
    <script>
        // Initialize Lucide icons inside canvas
        if (typeof lucide !== 'undefined') { lucide.createIcons(); }
    <\/script>
    <script>${js}<\/script>
</body>
</html>`;
}

function showCanvas(canvas) {
    state.currentCanvas = canvas;
    const canvasPanel = document.getElementById('canvasPanel');
    const canvasTitle = document.getElementById('canvasTitle');
    const canvasFrame = document.getElementById('canvasFrame');
    const chatColumn = document.getElementById('chatColumn');

    canvasTitle.textContent = canvas.name || 'Canvas';

    // Clean up any site preview buttons (regular canvas doesn't need them)
    updateCanvasButtons(false);

    // Show canvas panel as right 2/3
    canvasPanel.classList.remove('hidden');

    // Constrain chat column to 1/3
    chatColumn.style.flex = '0 0 33.333%';
    chatColumn.style.maxWidth = '33.333%';

    // Close sidebar if open (since canvas takes focus)
    closeSidebar();

    // Hide footer text when compact
    const footer = document.getElementById('chatFooter');
    if (footer) footer.classList.add('hidden');

    // Build the canvas HTML
    // Prefer server-rendered content (Tera templates resolved) over raw templates
    const html = canvas._rendered_html || canvas.html_content || canvas.html || '';
    const css = canvas._rendered_css || canvas.css_content || canvas.css || '';
    const js = canvas._rendered_js || canvas.js_content || canvas.javascript || '';

    const canvasHtml = buildCanvasDocument(canvas.name || 'Canvas', html, css, js);

    // Load into iframe
    const blob = new Blob([canvasHtml], { type: 'text/html' });
    const url = URL.createObjectURL(blob);
    canvasFrame.src = url;

    // Clean up blob URL after load
    canvasFrame.onload = () => {
        setTimeout(() => URL.revokeObjectURL(url), 100);
    };

    // Navigate to chat view so the conversation is visible alongside canvas
    navigate('chat');
    saveState();
    lucide.createIcons();
}

function closeCanvas() {
    const canvasPanel = document.getElementById('canvasPanel');
    const chatColumn = document.getElementById('chatColumn');

    // Hide canvas panel
    canvasPanel.classList.add('hidden');

    // Restore chat column to full width
    chatColumn.style.flex = '';
    chatColumn.style.maxWidth = '';

    // Restore footer
    const footer = document.getElementById('chatFooter');
    if (footer) footer.classList.remove('hidden');

    // Clean up site preview buttons
    updateCanvasButtons(false);

    // Clear iframe src to stop any running content
    document.getElementById('canvasFrame').src = 'about:blank';

    state.currentCanvas = null;
    state.currentSystemCanvasId = null;

    // Clear nav highlights
    document.querySelectorAll('.nav-item').forEach(el => el.classList.remove('active'));

    saveState();
    lucide.createIcons();
}

/**
 * Open a live site preview in the canvas panel iframe.
 * Called when site tools (create_page, update_page, publish_site) complete.
 * Loads the actual site URL directly in the iframe so the user can interact
 * with their app while continuing to chat with AMOS.
 */
function openSitePreview(url, siteSlug) {
    const canvasPanel = document.getElementById('canvasPanel');
    const chatColumn = document.getElementById('chatColumn');
    const canvasFrame = document.getElementById('canvasFrame');
    const canvasTitle = document.getElementById('canvasTitle');

    // Store site preview state
    state.currentCanvas = { type: 'site_preview', slug: siteSlug, url: url };
    state.currentSystemCanvasId = null;

    // Set title with site slug
    canvasTitle.textContent = siteSlug ? `Site: ${siteSlug}` : 'Site Preview';

    // Show refresh and open-in-tab buttons
    updateCanvasButtons(true);

    // Load the site URL directly in the iframe
    canvasFrame.src = url;

    // Show canvas panel
    canvasPanel.classList.remove('hidden');

    // Constrain chat to 1/3 width
    chatColumn.style.flex = '0 0 33.333%';
    chatColumn.style.maxWidth = '33.333%';

    // Hide footer in compact mode
    const footer = document.getElementById('chatFooter');
    if (footer) footer.classList.add('hidden');

    // Close sidebar if open
    closeSidebar();

    lucide.createIcons();
}

/**
 * Refresh the site preview iframe (reload current URL).
 */
function refreshSitePreview() {
    const canvasFrame = document.getElementById('canvasFrame');
    if (state.currentCanvas && state.currentCanvas.type === 'site_preview') {
        canvasFrame.src = canvasFrame.src; // Force reload
    }
}

/**
 * Open the site preview in a new browser tab.
 */
function openSiteInNewTab() {
    if (state.currentCanvas && state.currentCanvas.type === 'site_preview') {
        window.open(state.currentCanvas.url, '_blank');
    }
}

/**
 * Open an app preview in the canvas panel.
 * Loads the canvas by ID and renders it as a freeform canvas (blob URL).
 */
async function openAppPreview(canvasId, appName) {
    try {
        const response = await fetch(`${state.apiBase}/api/v1/canvases/${canvasId}`);
        if (!response.ok) throw new Error('Failed to load app canvas');

        const canvas = await response.json();

        // Try to get rendered content from the API
        try {
            const renderResp = await fetch(`${state.apiBase}/api/v1/canvases/${canvasId}/render`);
            if (renderResp.ok) {
                const rendered = await renderResp.json();
                canvas._rendered_html = rendered.content;
                canvas._rendered_js = rendered.js_content;
                canvas._rendered_css = rendered.css_content;
            }
        } catch (renderErr) {
            console.warn('App canvas render failed, using raw content:', renderErr);
        }

        canvas.name = appName || canvas.name;
        showCanvas(canvas);
        navigate('chat');
    } catch (err) {
        console.error('Error loading app preview:', err);
    }
}

// ============================================================================
// Plan Mode
// ============================================================================

/**
 * Toggle plan mode on/off.
 */
function togglePlanMode() {
    state.planMode = !state.planMode;
    updatePlanModeUI();
}

/**
 * Update the plan mode UI indicator.
 */
function updatePlanModeUI() {
    const btn = document.getElementById('planModeBtn');
    const badge = document.getElementById('planModeBadge');
    if (btn) {
        if (state.planMode) {
            btn.classList.add('text-amber-500');
            btn.classList.remove('text-gray-400');
            btn.title = 'Plan Mode ON - Click to disable';
        } else {
            btn.classList.remove('text-amber-500');
            btn.classList.add('text-gray-400');
            btn.title = 'Plan Mode OFF - Click to enable';
        }
    }
    if (badge) {
        badge.classList.toggle('hidden', !state.planMode);
    }
}

/**
 * Update canvas panel header buttons based on content type.
 * Site previews get refresh + open-in-tab buttons.
 */
function updateCanvasButtons(isSitePreview) {
    const btnContainer = document.querySelector('#canvasPanel .flex-shrink-0 .flex.items-center');
    if (!btnContainer) return;

    // Remove any existing site preview buttons
    btnContainer.querySelectorAll('.site-preview-btn').forEach(el => el.remove());

    if (isSitePreview) {
        // Add refresh button
        const refreshBtn = document.createElement('button');
        refreshBtn.className = 'site-preview-btn p-1 rounded hover:bg-gray-200 dark:hover:bg-gray-700';
        refreshBtn.title = 'Refresh preview';
        refreshBtn.onclick = refreshSitePreview;
        refreshBtn.innerHTML = '<i data-lucide="refresh-cw" class="w-4 h-4"></i>';

        // Add open-in-tab button
        const openBtn = document.createElement('button');
        openBtn.className = 'site-preview-btn p-1 rounded hover:bg-gray-200 dark:hover:bg-gray-700';
        openBtn.title = 'Open in new tab';
        openBtn.onclick = openSiteInNewTab;
        openBtn.innerHTML = '<i data-lucide="external-link" class="w-4 h-4"></i>';

        // Insert before expand and close buttons
        btnContainer.prepend(openBtn);
        btnContainer.prepend(refreshBtn);
    }
}

function openSidebar() {
    document.getElementById('sidebar').classList.add('sidebar-open');
    document.getElementById('sidebarBackdrop').classList.add('active');
}

function closeSidebar() {
    document.getElementById('sidebar').classList.remove('sidebar-open');
    document.getElementById('sidebarBackdrop').classList.remove('active');
}

function toggleSidebar() {
    const sidebar = document.getElementById('sidebar');
    if (sidebar.classList.contains('sidebar-open')) {
        closeSidebar();
    } else {
        openSidebar();
    }
}

function expandCanvas() {
    if (!state.currentCanvas) return;

    // Open in new window — prefer rendered content
    const eHtml = state.currentCanvas._rendered_html || state.currentCanvas.html_content || state.currentCanvas.html || '';
    const eCss = state.currentCanvas._rendered_css || state.currentCanvas.css_content || state.currentCanvas.css || '';
    const eJs = state.currentCanvas._rendered_js || state.currentCanvas.js_content || state.currentCanvas.javascript || '';

    const canvasHtml = buildCanvasDocument(state.currentCanvas.name || 'Canvas', eHtml, eCss, eJs);

    const newWindow = window.open('', '_blank');
    if (newWindow) {
        newWindow.document.write(canvasHtml);
        newWindow.document.close();
    }
}

async function openCanvas(canvasId) {
    try {
        const response = await fetch(`${state.apiBase}/api/v1/canvases/${canvasId}`);
        if (!response.ok) throw new Error('Failed to load canvas');

        const canvas = await response.json();

        // Render the canvas server-side (resolves Tera templates, fetches data)
        try {
            const renderResponse = await fetch(`${state.apiBase}/api/v1/canvases/${canvasId}/render`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ data_context: null })
            });
            if (renderResponse.ok) {
                const rendered = await renderResponse.json();
                canvas._rendered_html = rendered.content || '';
                canvas._rendered_css = rendered.css_content || '';
                canvas._rendered_js = rendered.js_content || '';
            }
        } catch (renderErr) {
            console.warn('Canvas render failed, using raw content:', renderErr);
        }

        showCanvas(canvas);
        navigate('chat');
    } catch (err) {
        alert('Error loading canvas: ' + err.message);
    }
}

async function deleteCanvas(canvasId) {
    if (!confirm('Are you sure you want to delete this canvas?')) return;

    try {
        const response = await fetch(`${state.apiBase}/api/v1/canvases/${canvasId}`, {
            method: 'DELETE'
        });
        if (!response.ok) throw new Error('Failed to delete canvas');

        // If we're viewing the canvases system canvas, reload it
        if (state.currentSystemCanvasId) {
            openSystemCanvas(state.currentSystemCanvasId);
        }
    } catch (err) {
        alert('Error deleting canvas: ' + err.message);
    }
}

function createCanvas() {
    navigate('chat');
    sendQuickMessage('Create a new canvas for me');
}

// ============================================================================
// File Upload & Attachment Functions
// ============================================================================

/**
 * Handle file input change (from paperclip button).
 */
function handleFileSelect(event) {
    const files = event.target.files;
    if (!files || files.length === 0) return;
    for (const file of files) {
        processFile(file);
    }
    // Reset the input so the same file can be selected again
    event.target.value = '';
}

/**
 * Handle paste events on the textarea (screenshots, images).
 */
function handlePaste(event) {
    const items = event.clipboardData?.items;
    if (!items) return;

    for (const item of items) {
        if (item.type.startsWith('image/')) {
            event.preventDefault();
            const file = item.getAsFile();
            if (file) {
                // Give pasted images a friendly name
                const ext = file.type.split('/')[1] || 'png';
                const namedFile = new File([file], `pasted-image-${Date.now()}.${ext}`, { type: file.type });
                processFile(namedFile);
            }
            return; // Only handle first image
        }
    }
    // If no image found, let the default paste behavior proceed (text paste)
}

/**
 * Handle drag over the input area.
 */
function handleDragOver(event) {
    event.preventDefault();
    event.stopPropagation();
    const overlay = document.getElementById('dragOverlay');
    if (overlay) overlay.classList.remove('hidden');
    const inputArea = document.getElementById('chatInputArea');
    if (inputArea) inputArea.classList.add('drag-active');
}

/**
 * Handle drag leave the input area.
 */
function handleDragLeave(event) {
    event.preventDefault();
    event.stopPropagation();
    // Only hide if we're actually leaving the drop zone (not entering a child)
    const inputArea = document.getElementById('chatInputArea');
    if (inputArea && !inputArea.contains(event.relatedTarget)) {
        const overlay = document.getElementById('dragOverlay');
        if (overlay) overlay.classList.add('hidden');
        inputArea.classList.remove('drag-active');
    }
}

/**
 * Handle drop on the input area.
 */
function handleDrop(event) {
    event.preventDefault();
    event.stopPropagation();

    const overlay = document.getElementById('dragOverlay');
    if (overlay) overlay.classList.add('hidden');
    const inputArea = document.getElementById('chatInputArea');
    if (inputArea) inputArea.classList.remove('drag-active');

    const files = event.dataTransfer?.files;
    if (!files || files.length === 0) return;

    for (const file of files) {
        processFile(file);
    }
}

/**
 * Process a file: generate local preview, upload to server, add to pending.
 */
async function processFile(file) {
    // Validate size (20MB limit to match server)
    const MAX_SIZE = 20 * 1024 * 1024;
    if (file.size > MAX_SIZE) {
        alert(`File "${file.name}" is too large (max 20 MB).`);
        return;
    }

    // Generate local preview for images
    let localPreview = null;
    if (file.type.startsWith('image/')) {
        localPreview = await createImagePreview(file);
    }

    // Create a temporary placeholder in the preview strip
    const tempId = 'temp-' + Date.now() + '-' + Math.random().toString(36).slice(2);
    addAttachmentPreview({
        id: tempId,
        filename: file.name,
        content_type: file.type,
        size_bytes: file.size,
        localPreview: localPreview,
        uploading: true,
    });

    try {
        const uploadResult = await uploadFile(file);

        // Replace temp attachment with real one
        const idx = state.pendingAttachments.findIndex(a => a.id === tempId);
        if (idx !== -1) {
            state.pendingAttachments[idx] = {
                id: uploadResult.id,
                filename: uploadResult.original_filename || file.name,
                content_type: uploadResult.content_type || file.type,
                size_bytes: uploadResult.size_bytes || file.size,
                url: uploadResult.url,
                localPreview: localPreview,
                uploading: false,
            };
        }

        // Update the preview chip to remove uploading state
        const chip = document.querySelector(`[data-attachment-id="${tempId}"]`);
        if (chip) {
            chip.setAttribute('data-attachment-id', uploadResult.id);
            chip.classList.remove('opacity-60');
            const spinner = chip.querySelector('.upload-spinner');
            if (spinner) spinner.remove();
            // Update remove button onclick
            const removeBtn = chip.querySelector('.remove-attachment');
            if (removeBtn) removeBtn.onclick = () => removeAttachment(uploadResult.id);
        }
    } catch (err) {
        console.error('Upload failed:', err);
        // Remove the temp attachment
        removeAttachment(tempId);
        alert(`Failed to upload "${file.name}": ${err.message}`);
    }
}

/**
 * Create a data URL preview for an image file.
 */
function createImagePreview(file) {
    return new Promise((resolve) => {
        const reader = new FileReader();
        reader.onload = (e) => resolve(e.target.result);
        reader.onerror = () => resolve(null);
        reader.readAsDataURL(file);
    });
}

/**
 * Upload a file to the server via multipart POST.
 * Returns the upload metadata object from the server.
 */
async function uploadFile(file) {
    const formData = new FormData();
    formData.append('file', file);
    if (state.sessionId) {
        formData.append('session_id', state.sessionId);
    }
    formData.append('context', 'chat');

    const response = await fetch(`${state.apiBase}/api/v1/uploads`, {
        method: 'POST',
        body: formData,
    });

    if (!response.ok) {
        const errText = await response.text();
        throw new Error(`Upload failed (${response.status}): ${errText}`);
    }

    return await response.json();
}

/**
 * Add a preview chip to the attachment preview strip.
 */
function addAttachmentPreview(attachment) {
    // Add to state
    state.pendingAttachments.push(attachment);

    const container = document.getElementById('attachmentPreview');
    container.classList.remove('hidden');

    const chip = document.createElement('div');
    chip.setAttribute('data-attachment-id', attachment.id);
    chip.className = `inline-flex items-center gap-2 px-3 py-1.5 rounded-lg border border-gray-200 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm ${attachment.uploading ? 'opacity-60' : ''}`;

    const isImage = attachment.content_type && attachment.content_type.startsWith('image/');

    let chipContent = '';

    // Show image thumbnail or file icon
    if (isImage && attachment.localPreview) {
        chipContent += `<img src="${attachment.localPreview}" alt="" class="w-8 h-8 rounded object-cover">`;
    } else {
        chipContent += `<i data-lucide="file" class="w-4 h-4 text-gray-400"></i>`;
    }

    // Filename (truncated)
    const displayName = attachment.filename.length > 20
        ? attachment.filename.slice(0, 17) + '...'
        : attachment.filename;
    chipContent += `<span class="truncate max-w-[120px]">${escapeHtml(displayName)}</span>`;

    // Upload spinner (if uploading)
    if (attachment.uploading) {
        chipContent += `<div class="upload-spinner spinner" style="width:12px;height:12px;border-width:1.5px;"></div>`;
    }

    // Remove button
    chipContent += `<button class="remove-attachment p-0.5 rounded hover:bg-gray-200 dark:hover:bg-gray-600" onclick="removeAttachment('${attachment.id}')" title="Remove">
        <i data-lucide="x" class="w-3 h-3"></i>
    </button>`;

    chip.innerHTML = chipContent;
    container.appendChild(chip);
    lucide.createIcons();
}

/**
 * Remove an attachment from pending list and preview strip.
 */
function removeAttachment(id) {
    state.pendingAttachments = state.pendingAttachments.filter(a => a.id !== id);

    const chip = document.querySelector(`[data-attachment-id="${id}"]`);
    if (chip) chip.remove();

    // Hide the preview strip if no attachments remain
    if (state.pendingAttachments.length === 0) {
        const container = document.getElementById('attachmentPreview');
        container.classList.add('hidden');
    }
}

/**
 * Clear all attachment previews (called after sending a message).
 */
function clearAttachmentPreview() {
    state.pendingAttachments = [];
    const container = document.getElementById('attachmentPreview');
    if (container) {
        container.innerHTML = '';
        container.classList.add('hidden');
    }
}

/**
 * Format file size for display.
 */
function formatFileSize(bytes) {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
}

// ============================================================================
// Utility Functions
// ============================================================================

function autoResize(textarea) {
    textarea.style.height = 'auto';
    textarea.style.height = Math.min(textarea.scrollHeight, 200) + 'px';
}

function handleKeyDown(event) {
    if (event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
        sendMessage();
    }
}

function formatTimestamp(dateStr) {
    if (!dateStr) return 'Unknown';
    const date = new Date(dateStr);
    const now = new Date();
    const diff = now - date;

    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (seconds < 60) return 'Just now';
    if (minutes < 60) return `${minutes}m ago`;
    if (hours < 24) return `${hours}h ago`;
    if (days < 7) return `${days}d ago`;

    return date.toLocaleDateString();
}

// ============================================================================
// Specialist Sidebar
// ============================================================================

const specialistIcons = {
    'search': 'search',
    'graduation-cap': 'graduation-cap',
    'cpu': 'cpu',
};

/**
 * Load specialist status for the sidebar.
 * Polls GET /api/v1/harness/specialists and updates the specialist section.
 */
async function loadSpecialists() {
    const section = document.getElementById('specialistSection');
    const container = document.getElementById('specialistList');
    if (!section || !container) return;

    try {
        const response = await fetch(`${state.apiBase}/api/v1/harness/specialists`);
        if (!response.ok) {
            section.classList.add('hidden');
            return;
        }

        const data = await response.json();
        const active = data.specialists || [];
        const available = data.available || [];

        if (active.length === 0 && available.length === 0) {
            section.classList.add('hidden');
            return;
        }

        section.classList.remove('hidden');
        let html = '';

        // Active specialists
        for (const spec of active) {
            const icon = specialistIcons[spec.icon_hint] || 'cpu';
            const dotColor = spec.healthy
                ? 'bg-green-500'
                : (spec.status === 'running' ? 'bg-yellow-500 animate-pulse' : 'bg-red-500');

            html += `<div class="flex items-center gap-2 px-3 py-2 rounded-lg text-sm text-gray-700 dark:text-gray-300">
                <i data-lucide="${icon}" class="w-4 h-4 flex-shrink-0"></i>
                <span class="truncate flex-1">${escapeHtml(spec.friendly_name)}</span>
                <span class="w-2 h-2 rounded-full ${dotColor} flex-shrink-0"></span>
            </div>`;
        }

        // Available specialists (dimmed)
        for (const spec of available) {
            const icon = specialistIcons[spec.icon_hint] || 'cpu';
            html += `<div class="flex items-center gap-2 px-3 py-2 rounded-lg text-sm text-gray-400 dark:text-gray-600" title="${escapeHtml(spec.description || '')}">
                <i data-lucide="${icon}" class="w-4 h-4 flex-shrink-0"></i>
                <span class="truncate flex-1">${escapeHtml(spec.friendly_name)}</span>
                <span class="text-xs">available</span>
            </div>`;
        }

        container.innerHTML = html;
        lucide.createIcons();
    } catch (e) {
        // Silently hide section if endpoint unavailable
        section.classList.add('hidden');
    }
}

// Start specialist polling (every 30 seconds)
loadSpecialists();
setInterval(loadSpecialists, 30000);

// ============================================================================
// Wallet Integration
// ============================================================================

/**
 * Show the wallet settings modal.
 * Called from the sidebar wallet indicator and header indicator.
 */
function showWalletSettings() {
    showWalletModal();
}

// Note: showWalletModal(), closeWalletModal(), and copyWalletAddress() are
// defined in wallet.js and loaded before app.js.

// Refresh wallet balance periodically (every 60 seconds) when connected
setInterval(function() {
    if (typeof AMOSWallet !== 'undefined' && AMOSWallet.connected) {
        AMOSWallet.refreshBalance();
    }
}, 60000);
