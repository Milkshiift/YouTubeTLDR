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
        summaryTitleText: document.getElementById('summary-title-text'),
        summaryOutput: document.getElementById('summary-output'),
        transcriptSection: document.getElementById('transcript-section'),
        transcriptText: document.getElementById('transcript-text'),
        copySummaryBtn: document.getElementById('copy-summary-btn'),
        copyTranscriptBtn: document.getElementById('copy-transcript-btn'),
        videoLink: document.getElementById('video-link'),
    };

    const state = {
        summaries: [],
        activeSummaryIndex: -1,
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

            dom.copySummaryBtn.addEventListener('click', (e) => this.handleCopyClick(e, dom.summaryOutput.mdContent, dom.copySummaryBtn));
            dom.copyTranscriptBtn.addEventListener('click', (e) => this.handleCopyClick(e, dom.transcriptText.textContent, dom.copyTranscriptBtn));

            [dom.menuToggleBtn, dom.closeSidebarBtn, dom.sidebarOverlay].forEach(el => {
                if (el) el.addEventListener('click', () => this.toggleSidebar());
            });

            [dom.apiKey, dom.model, dom.systemPrompt].forEach(el => el.addEventListener('change', this.saveSettings));
            [dom.dryRun, dom.transcriptOnly].forEach(el => el.addEventListener('change', this.saveSettings));
        },

        loadSummaries() {
            state.summaries = JSON.parse(localStorage.getItem(config.storageKeys.summaries)) || [];
            if (state.summaries.length > 0) {
                state.activeSummaryIndex = 0;
            }
        },

        saveSummaries() {
            localStorage.setItem(config.storageKeys.summaries, JSON.stringify(state.summaries));
            this.render();
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
            const url = dom.urlInput.value.trim();
            if (!url) {
                state.error = "Please enter a YouTube URL.";
                this.render();
                return;
            }

            this.saveSettings();
            state.isLoading = true;
            state.error = null;
            state.activeSummaryIndex = -1;
            this.render();

            try {
                const response = await fetch(`${config.baseURL}/api/summarize`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        url,
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
                        // Not JSON, or JSON without .error, use text as is.
                    }
                    throw new Error(errorMsg || `Server error: ${response.status}`);
                }

                const data = JSON.parse(responseText);

                const newSummary = {
                    name: data.video_name,
                    summary: data.summary,
                    transcript: data.subtitles,
                    url: url
                };

                state.summaries.unshift(newSummary);
                state.activeSummaryIndex = 0;

            } catch (error) {
                console.error('Summarization failed:', error);
                state.error = error.message;
            } finally {
                state.isLoading = false;
                this.saveSummaries();
            }
        },

        handleNewSummary() {
            state.activeSummaryIndex = -1;
            state.error = null;
            dom.urlInput.value = '';
            this.render();
            if (this.isMobile()) this.toggleSidebar(false);
        },

        handleClearSummaries() {
            if (confirm('Are you sure you want to clear all saved summaries?')) {
                state.summaries = [];
                state.activeSummaryIndex = -1;
                state.error = null;
                this.saveSummaries();
            }
        },

        handleSidebarClick(e) {
            const link = e.target.closest('a[data-index]');
            const deleteBtn = e.target.closest('button[data-index]');

            if (deleteBtn) {
                e.preventDefault();
                const index = parseInt(deleteBtn.dataset.index, 10);
                this.deleteSummary(index);
                return;
            }

            if (link) {
                e.preventDefault();
                state.activeSummaryIndex = parseInt(link.dataset.index, 10);
                state.error = null;
                this.render();
                if (this.isMobile()) this.toggleSidebar(false);
            }
        },

        deleteSummary(indexToDelete) {
            const summaryToDelete = state.summaries[indexToDelete];
            if (!summaryToDelete) return;

            if (confirm(`Are you sure you want to delete the summary for "${summaryToDelete.name}"?`)) {
                state.summaries.splice(indexToDelete, 1);

                if (state.activeSummaryIndex === indexToDelete) {
                    state.activeSummaryIndex = -1;
                    state.error = null; // Clear error if the active (and possibly error-causing) summary is deleted
                } else if (state.activeSummaryIndex > indexToDelete) {
                    state.activeSummaryIndex--;
                }

                this.saveSummaries();
            }
        },

        render() {
            const hasActiveSummary = state.activeSummaryIndex > -1;
            const currentSummary = hasActiveSummary ? state.summaries[state.activeSummaryIndex] : null;
            const shouldShowSummaryView = state.isLoading || hasActiveSummary || state.error;

            dom.welcomeView.classList.toggle('hidden', shouldShowSummaryView);
            dom.summaryView.classList.toggle('hidden', !shouldShowSummaryView);

            const hasStatus = state.isLoading || state.error;
            dom.statusContainer.classList.toggle('hidden', !hasStatus);
            dom.loader.style.display = state.isLoading ? 'flex' : 'none';
            dom.errorMessage.style.display = state.error ? 'block' : 'none';
            dom.errorMessage.textContent = state.error || '';

            dom.summaryContainer.classList.toggle('hidden', !currentSummary || hasStatus);
            dom.transcriptSection.classList.toggle('hidden', true);

            if (currentSummary) {
                dom.summaryTitleText.textContent = currentSummary.name;
                dom.videoLink.href = currentSummary.url;
                dom.summaryOutput.mdContent = currentSummary.summary;
                if (currentSummary.transcript && currentSummary.transcript.trim()) {
                    dom.transcriptText.textContent = currentSummary.transcript;
                    dom.transcriptSection.classList.remove('hidden');
                }
            }

            this.renderSidebarList();
            if (window.lucide) {
                lucide.createIcons();
            }
        },

        renderSidebarList() {
            dom.savedSummariesList.innerHTML = state.summaries.map((summary, index) => `
                <li class="${index === state.activeSummaryIndex ? 'active' : ''}">
                    <a href="#" data-index="${index}" title="${this.escapeHtml(summary.name)}">
                        <i data-lucide="file-text"></i>
                        <span>${this.escapeHtml(summary.name)}</span>
                    </a>
                    <button class="delete-summary-btn" data-index="${index}" title="Delete summary">
                        <i data-lucide="trash-2"></i>
                    </button>
                </li>
            `).join('');
        },

        async handleCopyClick(e, text, button) {
            e.preventDefault();
            e.stopPropagation();
            if (!text) return;

            const originalIcon = button.innerHTML;
            const originalTitle = button.title;
            try {
                await navigator.clipboard.writeText(text);
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