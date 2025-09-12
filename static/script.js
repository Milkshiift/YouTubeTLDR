document.addEventListener('DOMContentLoaded', () => {
    const config = {
        baseURL: `${location.protocol}//${location.hostname}${location.port ? ':' + location.port : ''}`,
        storageKeys: {
            apiKey: 'youtube-tldr-api-key',
            model: 'youtube-tldr-model',
            language: 'youtube-tldr-language',
            systemPrompt: 'youtube-tldr-system-prompt',
            dryRun: 'youtube-tldr-dry-run',
            transcriptOnly: 'youtube-tldr-transcript-only',
            summaries: 'youtube-tldr-summaries'
        },
        defaults: {
            model: 'gemini-2.5-flash',
            systemPrompt: "You are an expert video summarizer specializing in creating structured, accurate overviews. Given a YouTube video transcript, extract and present the most crucial information in an article-style format. Prioritize fidelity to the original content, ensuring all significant points, arguments, and key details are faithfully represented. Organize the summary logically with clear, descriptive headings and/or concise bullet points. For maximum skim-readability, bold key terms, core concepts, and critical takeaways within the text. Eliminate advertisements, sponsorships, conversational filler, repeated phrases, and irrelevant tangents, but retain all essential content.",
            language: 'en'
        }
    };

    const dom = {
        // Settings
        apiKey: document.getElementById('api-key'),
        model: document.getElementById('model'),
        language: document.getElementById('language'),
        systemPrompt: document.getElementById('system-prompt'),
        dryRun: document.getElementById('dry-run'),
        transcriptOnly: document.getElementById('transcript-only'),
        // Sidebar
        sidebar: document.getElementById('sidebar'),
        newSummaryBtn: document.getElementById('new-summary-btn'),
        savedSummariesList: document.getElementById('saved-summaries-list'),
        clearSummariesBtn: document.getElementById('clear-summaries-btn'),
        menuToggleBtn: document.getElementById('menu-toggle-btn'),
        closeSidebarBtn: document.getElementById('close-sidebar-btn'),
        sidebarOverlay: document.getElementById('sidebar-overlay'),
        // Main View
        mainContent: document.getElementById('main-content'),
        welcomeView: document.getElementById('welcome-view'),
        summaryView: document.getElementById('summary-view'),
        form: document.getElementById('summary-form'),
        urlInput: document.getElementById('youtube-url'),
        // Status & Output
        statusContainer: document.getElementById('status-container'),
        loader: document.getElementById('loader'),
        errorMessage: document.getElementById('error-message'),
        summaryContainer: document.getElementById('summary-container'),
    };

    const state = {
        summaries: [], // Persisted history of all summaries
        currentBatch: [], // Summaries currently being displayed in the main view
        isLoading: false,
        error: null,
    };

    const app = {
        init() {
            this.loadSettings();
            this.loadSummaries();
            this.addEventListeners();
            this.render();
        },

        addEventListeners() {
            dom.form.addEventListener('submit', this.handleFormSubmit.bind(this));
            dom.clearSummariesBtn.addEventListener('click', this.handleClearSummaries.bind(this));
            dom.newSummaryBtn.addEventListener('click', this.handleNewSummary.bind(this));
            dom.savedSummariesList.addEventListener('click', this.handleSidebarClick.bind(this));

            // Event delegation for dynamically created elements
            dom.summaryContainer.addEventListener('click', (e) => {
                const copySummaryBtn = e.target.closest('.copy-summary-btn');
                const copyTranscriptBtn = e.target.closest('.copy-transcript-btn');
                const downloadBtn = e.target.closest('#download-all-btn');

                if (copySummaryBtn) {
                    const summaryOutput = copySummaryBtn.closest('.summary-item').querySelector('.summary-output');
                    this.handleCopyClick(e, summaryOutput.mdContent, copySummaryBtn);
                }
                if (copyTranscriptBtn) {
                    const transcriptText = copyTranscriptBtn.closest('.transcript-section').querySelector('.transcript-text');
                    this.handleCopyClick(e, transcriptText.textContent, copyTranscriptBtn);
                }
                if(downloadBtn) {
                    this.handleDownloadAll();
                }
            });

            [dom.menuToggleBtn, dom.closeSidebarBtn, dom.sidebarOverlay].forEach(el => {
                if (el) el.addEventListener('click', () => this.toggleSidebar());
            });

            [dom.apiKey, dom.model, dom.language, dom.systemPrompt].forEach(el => el.addEventListener('change', this.saveSettings));
            [dom.dryRun, dom.transcriptOnly].forEach(el => el.addEventListener('change', this.saveSettings));
        },

        loadSummaries() {
            state.summaries = JSON.parse(localStorage.getItem(config.storageKeys.summaries)) || [];
            this.renderSidebarList();
        },

        saveSummaries() {
            localStorage.setItem(config.storageKeys.summaries, JSON.stringify(state.summaries));
            this.renderSidebarList();
        },

        loadSettings() {
            dom.apiKey.value = localStorage.getItem(config.storageKeys.apiKey) || '';
            dom.model.value = localStorage.getItem(config.storageKeys.model) || config.defaults.model;
            dom.language.value = localStorage.getItem(config.storageKeys.language) || config.defaults.language;
            dom.systemPrompt.value = localStorage.getItem(config.storageKeys.systemPrompt) || config.defaults.systemPrompt;
            dom.dryRun.checked = localStorage.getItem(config.storageKeys.dryRun) === 'true';
            dom.transcriptOnly.checked = localStorage.getItem(config.storageKeys.transcriptOnly) === 'true';
        },

        saveSettings() {
            localStorage.setItem(config.storageKeys.apiKey, dom.apiKey.value);
            localStorage.setItem(config.storageKeys.model, dom.model.value);
            localStorage.setItem(config.storageKeys.language, dom.language.value);
            localStorage.setItem(config.storageKeys.systemPrompt, dom.systemPrompt.value);
            localStorage.setItem(config.storageKeys.dryRun, dom.dryRun.checked);
            localStorage.setItem(config.storageKeys.transcriptOnly, dom.transcriptOnly.checked);
        },

        async handleFormSubmit(event) {
            event.preventDefault();
            const urls = dom.urlInput.value.trim().split('\n').map(url => url.trim()).filter(url => url);
            if (urls.length === 0) {
                state.error = "Please enter at least one YouTube URL.";
                this.render();
                return;
            }

            this.saveSettings();
            state.isLoading = true;
            state.error = null;
            state.currentBatch = [];
            this.render();

            try {
                const response = await fetch(`${config.baseURL}/api/summarize`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        urls,
                        api_key: dom.apiKey.value,
                        model: dom.model.value,
                        language: dom.language.value,
                        system_prompt: dom.systemPrompt.value,
                        dry_run: dom.dryRun.checked,
                        transcript_only: dom.transcriptOnly.checked,
                    }),
                });

                const responseText = await response.text();

                if (!response.ok) {
                    let errorMsg = responseText;
                    try {
                        const errorData = JSON.parse(responseText);
                        if (errorData && errorData.error) {
                            errorMsg = errorData.error;
                        }
                    } catch (e) {
                        // Not JSON, use text as is.
                    }
                    throw new Error(errorMsg || `Server error: ${response.status}`);
                }

                const results = JSON.parse(responseText);
                state.currentBatch = results.map((data, index) => ({
                    name: data.video_name,
                    summary: data.summary,
                    transcript: data.subtitles,
                    url: urls.find(u => u === data.url) // Ensure correct URL is mapped
                }));

                // Add the new batch to the top of the history
                state.summaries.unshift(...state.currentBatch);
                this.saveSummaries();

            } catch (error) {
                console.error('Summarization failed:', error);
                state.error = error.message;
            } finally {
                state.isLoading = false;
                this.render();
            }
        },

        async handleDownloadAll() {
            if (state.currentBatch.length === 0) return;

            try {
                const response = await fetch(`${config.baseURL}/api/download`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(state.currentBatch)
                });

                if (!response.ok) {
                    const errorText = await response.text();
                    throw new Error(`Download failed: ${errorText || response.statusText}`);
                }

                const blob = await response.blob();
                const url = window.URL.createObjectURL(blob);
                const a = document.createElement('a');
                a.style.display = 'none';
                a.href = url;
                a.download = 'summaries.zip';
                document.body.appendChild(a);
                a.click();
                window.URL.revokeObjectURL(url);
                a.remove();

            } catch (error) {
                console.error('Download failed:', error);
                state.error = `Download failed: ${error.message}`;
                this.render();
            }
        },

        handleNewSummary() {
            state.currentBatch = [];
            state.error = null;
            dom.urlInput.value = '';
            this.render();
            if (this.isMobile()) this.toggleSidebar(false);
        },

        handleClearSummaries() {
            if (confirm('Are you sure you want to clear all saved summaries?')) {
                state.summaries = [];
                state.currentBatch = [];
                state.error = null;
                this.saveSummaries();
                this.render();
            }
        },

        handleSidebarClick(e) {
            const link = e.target.closest('a[data-index]');
            const deleteBtn = e.target.closest('button[data-index]');

            if (deleteBtn) {
                e.preventDefault();
                e.stopPropagation();
                const index = parseInt(deleteBtn.dataset.index, 10);
                this.deleteSummary(index);
                return;
            }

            if (link) {
                e.preventDefault();
                const index = parseInt(link.dataset.index, 10);
                // When a history item is clicked, display just that one.
                state.currentBatch = [state.summaries[index]];
                state.error = null;
                this.render();
                if (this.isMobile()) this.toggleSidebar(false);
            }
        },

        deleteSummary(indexToDelete) {
            const summaryToDelete = state.summaries[indexToDelete];
            if (!summaryToDelete) return;

            if (confirm(`Are you sure you want to delete the summary for "${summaryToDelete.name}"?`)) {
                // If the item being deleted is the one currently displayed, clear the view.
                if (state.currentBatch.length === 1 && state.currentBatch[0].url === summaryToDelete.url) {
                    state.currentBatch = [];
                }

                state.summaries.splice(indexToDelete, 1);
                this.saveSummaries();
                this.render();
            }
        },

        render() {
            const hasBatchResults = state.currentBatch.length > 0;
            const shouldShowSummaryView = state.isLoading || hasBatchResults || state.error;

            dom.welcomeView.classList.toggle('hidden', shouldShowSummaryView);
            dom.summaryView.classList.toggle('hidden', !shouldShowSummaryView);

            const hasStatus = state.isLoading || state.error;
            dom.statusContainer.classList.toggle('hidden', !hasStatus);
            dom.loader.style.display = state.isLoading ? 'flex' : 'none';
            dom.errorMessage.style.display = state.error ? 'block' : 'none';
            dom.errorMessage.textContent = state.error || '';

            dom.summaryContainer.classList.toggle('hidden', !hasBatchResults || hasStatus);
            dom.summaryContainer.innerHTML = ''; // Clear previous results

            if (hasBatchResults) {
                if (state.currentBatch.length > 0) {
                    const downloadBtnHTML = `
                        <div class="batch-actions">
                            <button id="download-all-btn" class="primary-btn">
                                <i data-lucide="download"></i> Download All (${state.currentBatch.length} summaries)
                            </button>
                        </div>`;
                    dom.summaryContainer.innerHTML += downloadBtnHTML;
                }

                state.currentBatch.forEach(summary => {
                    const summaryEl = this.createSummaryElement(summary);
                    dom.summaryContainer.appendChild(summaryEl);
                });
            }

            this.renderSidebarList();

            if (window.lucide) {
                lucide.createIcons();
            }
        },

        createSummaryElement(summary) {
            const div = document.createElement('div');
            div.className = 'summary-item';

            const transcriptHTML = summary.transcript && summary.transcript.trim() ? `
                <div class="transcript-section">
                    <details class="transcript-details">
                        <summary>
                            <span><i data-lucide="scroll-text"></i> View Raw Transcript</span>
                            <span class="summary-actions">
                                <button class="copy-transcript-btn icon-btn" title="Copy Transcript"><i data-lucide="copy"></i></button>
                                <i data-lucide="chevron-down" class="chevron"></i>
                            </span>
                        </summary>
                        <pre class="transcript-text">${this.escapeHtml(summary.transcript)}</pre>
                    </details>
                </div>
            ` : '';

            div.innerHTML = `
                <h2 class="summary-title">
                    <i data-lucide="file-text"></i>
                    <span class="summary-title-text">${this.escapeHtml(summary.name)}</span>
                    <div style="width: min-content; display: inline-flex; gap: 0.5rem;">
                        <a href="${summary.url}" target="_blank" class="icon-btn" title="View Video"><i data-lucide="video"></i></a>
                        <button class="copy-summary-btn icon-btn" title="Copy Summary"><i data-lucide="copy"></i></button>
                    </div>
                </h2>
                <md-block class="summary-output">${summary.summary}</md-block>
                ${transcriptHTML}
            `;

            // Manually set mdContent for the md-block
            div.querySelector('.summary-output').mdContent = summary.summary;
            return div;
        },

        renderSidebarList() {
            dom.savedSummariesList.innerHTML = state.summaries.map((summary, index) => {
                // Check if the current history item is being displayed in the main view
                const isViewing = state.currentBatch.length === 1 && state.currentBatch[0].url === summary.url;
                return `
                <li class="${isViewing ? 'active' : ''}">
                    <a href="#" data-index="${index}" title="${this.escapeHtml(summary.name)}">
                        <i data-lucide="file-text"></i>
                        <span>${this.escapeHtml(summary.name)}</span>
                    </a>
                    <button class="delete-summary-btn" data-index="${index}" title="Delete summary">
                        <i data-lucide="trash-2"></i>
                    </button>
                </li>
            `}).join('');
        },

        async handleCopyClick(e, text, button) {
            e.preventDefault();
            e.stopPropagation();
            if (!text) return;

            const originalIcon = button.innerHTML;
            const originalTitle = button.title;
            try {
                await copyToClipboard(text);
                button.innerHTML = '<i data-lucide="check"></i>';
                button.title = 'Copied!';
                if (window.lucide) lucide.createIcons();
            } catch (err) {
                console.error('Failed to copy: ', err);
                button.title = 'Failed to copy';
            } finally {
                setTimeout(() => {
                    button.innerHTML = originalIcon;
                    button.title = originalTitle;
                    if (window.lucide) lucide.createIcons();
                }, 2000);
            }
        },

        isMobile: () => window.innerWidth <= 800,

        toggleSidebar(force) {
            document.body.classList.toggle('sidebar-open', force);
            dom.menuToggleBtn.setAttribute('aria-expanded', document.body.classList.contains('sidebar-open'));
        },

        escapeHtml(str) {
            const p = document.createElement('p');
            p.textContent = str;
            return p.innerHTML;
        }
    };

    app.init();
});

window.addEventListener('unhandledrejection', event => {
    console.error('Unhandled rejection:', event.reason);
});
window.addEventListener('error', event => {
    console.error('Uncaught error:', event.error);
});

async function copyToClipboard(textToCopy) {
    if (navigator.clipboard && window.isSecureContext) {
        await navigator.clipboard.writeText(textToCopy);
    } else {
        const textArea = document.createElement("textarea");
        textArea.value = textToCopy;
        textArea.style.position = "absolute";
        textArea.style.left = "-999999px";
        document.body.prepend(textArea);
        textArea.select();
        try {
            document.execCommand('copy');
        } catch (error) {
            console.error(error);
        } finally {
            textArea.remove();
        }
    }
}