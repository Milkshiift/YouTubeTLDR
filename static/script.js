document.addEventListener('DOMContentLoaded', () => {
    const apiKeyInput = document.getElementById('api-key');
    const modelInput = document.getElementById('model');
    const systemPromptInput = document.getElementById('system-prompt');
    const dryRunCheckbox = document.getElementById('dry-run');

    const sidebar = document.getElementById('sidebar');
    const newSummaryBtn = document.getElementById('new-summary-btn');
    const savedSummariesList = document.getElementById('saved-summaries-list');
    const clearSummariesBtn = document.getElementById('clear-summaries-btn');

    const mainContent = document.getElementById('main-content');
    const welcomeView = document.getElementById('welcome-view');
    const summaryView = document.getElementById('summary-view');

    const form = document.getElementById('summary-form');
    const urlInput = document.getElementById('youtube-url');

    const statusContainer = document.getElementById('status-container');
    const loader = document.getElementById('loader');
    const errorMessage = document.getElementById('error-message');

    const summaryContainer = document.getElementById('summary-container');
    const summaryTitle = document.getElementById('summary-title');
    const summaryOutput = document.getElementById('summary-output');

    const transcriptSection = document.getElementById('transcript-section');
    const transcriptText = document.getElementById('transcript-text');

    const baseURL = `${location.protocol}//${location.hostname}${location.port ? ':' + location.port : ''}`;

    const API_KEY_STORAGE_KEY = 'youtube-tldr-api-key';
    const MODEL_STORAGE_KEY = 'youtube-tldr-model';
    const SYSTEM_PROMPT_STORAGE_KEY = 'youtube-tldr-system-prompt';
    const DRY_RUN_STORAGE_KEY = 'youtube-tldr-dry-run';
    const SUMMARIES_STORAGE_KEY = 'youtube-tldr-summaries';

    const DEFAULT_SYSTEM_PROMPT = `You are an expert video summarizer. Create a structured, accurate overview of the provided transcript. Use clear headings for topics and bullet points for key details. Bold essential terms and concepts. Focus on the most important information, removing conversational filler.`;

    let activeSummaryIndex = -1;

    function getSummaries() {
        return JSON.parse(localStorage.getItem(SUMMARIES_STORAGE_KEY)) || [];
    }

    function saveSummaries(summaries) {
        localStorage.setItem(SUMMARIES_STORAGE_KEY, JSON.stringify(summaries));
        renderSavedSummaries();
    }

    function switchView(showSummary) {
        welcomeView.classList.toggle('hidden', showSummary);
        summaryView.classList.toggle('hidden', !showSummary);
    }

    function renderSavedSummaries() {
        const summaries = getSummaries();
        savedSummariesList.innerHTML = '';

        if (summaries.length > 0) {
            summaries.forEach((summary, index) => {
                const li = document.createElement('li');
                if (index === activeSummaryIndex) {
                    li.classList.add('active');
                }

                const a = document.createElement('a');
                a.href = '#';
                a.dataset.index = index;
                a.title = summary.url;

                const icon = document.createElement('i');
                icon.className = 'fas fa-file-alt';

                const text = document.createElement('span');
                text.textContent = summary.url;

                a.append(icon, text);

                const deleteBtn = document.createElement('button');
                deleteBtn.className = 'delete-summary-btn';
                deleteBtn.dataset.index = index;
                deleteBtn.innerHTML = '<i class="fas fa-trash-alt"></i>';
                deleteBtn.title = 'Delete summary';

                li.append(a, deleteBtn);
                savedSummariesList.appendChild(li);
            });
        }
    }

    function viewSummary(index) {
        const summaries = getSummaries();
        const summary = summaries[index];
        if (summary) {
            activeSummaryIndex = parseInt(index, 10);
            renderSavedSummaries();
            switchView(true);
            setStatus(false);
            summaryTitle.textContent = summary.url;
            summaryOutput.mdContent = summary.summary;
            summaryContainer.classList.remove('hidden');

            if (summary.transcript && summary.transcript.trim()) {
                transcriptText.textContent = summary.transcript;
                transcriptSection.classList.remove('hidden');
            } else {
                transcriptSection.classList.add('hidden');
            }
        }
    }

    function deleteSummary(indexToDelete) {
        const summaries = getSummaries();
        const summaryToDelete = summaries[indexToDelete];
        if (!summaryToDelete) return;

        if (confirm(`Are you sure you want to delete the summary for "${summaryToDelete.url}"?`)) {
            summaries.splice(indexToDelete, 1);

            if (activeSummaryIndex === indexToDelete) {
                activeSummaryIndex = -1;
                switchView(false);
            } else if (activeSummaryIndex > indexToDelete) {
                activeSummaryIndex--;
            }
            
            saveSummaries(summaries);
        }
    }

    function clearSummaries() {
        if (confirm('Are you sure you want to clear all saved summaries?')) {
            saveSummaries([]);
            activeSummaryIndex = -1;
            switchView(false);
        }
    }

    function saveSettings() {
        localStorage.setItem(API_KEY_STORAGE_KEY, apiKeyInput.value);
        localStorage.setItem(MODEL_STORAGE_KEY, modelInput.value);
        localStorage.setItem(SYSTEM_PROMPT_STORAGE_KEY, systemPromptInput.value);
        localStorage.setItem(DRY_RUN_STORAGE_KEY, dryRunCheckbox.checked);
    }

    function loadSettings() {
        apiKeyInput.value = localStorage.getItem(API_KEY_STORAGE_KEY) || '';
        modelInput.value = localStorage.getItem(MODEL_STORAGE_KEY) || 'gemini-1.5-flash-latest';
        systemPromptInput.value = localStorage.getItem(SYSTEM_PROMPT_STORAGE_KEY) || DEFAULT_SYSTEM_PROMPT;
        dryRunCheckbox.checked = localStorage.getItem(DRY_RUN_STORAGE_KEY) === 'true';
    }

    function setStatus(isLoading = false, error = null) {
        const hasStatus = isLoading || error;
        statusContainer.classList.toggle('hidden', !hasStatus);
        loader.style.display = isLoading ? 'flex' : 'none';
        errorMessage.style.display = error ? 'block' : 'none';
        errorMessage.textContent = error || '';

        if (hasStatus) {
            summaryContainer.classList.add('hidden');
            transcriptSection.classList.add('hidden');
        }
    }

    async function summarize(event) {
        event.preventDefault();
        const url = urlInput.value;
        if (!url) {
            setStatus(false, "Please enter a YouTube URL.");
            return;
        }

        saveSettings();
        switchView(true);
        setStatus(true);

        try {
            const response = await fetch(`${baseURL}/api/summarize`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    url,
                    api_key: apiKeyInput.value,
                    model: modelInput.value,
                    system_prompt: systemPromptInput.value,
                    dry_run: dryRunCheckbox.checked,
                }),
            });

            const data = await response.json();

            if (!response.ok) {
                throw new Error(data.error || `Server error: ${response.status}`);
            }

            setStatus(false);
            summaryTitle.textContent = url;
            summaryOutput.mdContent = data.summary;
            summaryContainer.classList.remove('hidden');

            if (data.subtitles && data.subtitles.trim()) {
                transcriptText.textContent = data.subtitles;
                transcriptSection.classList.remove('hidden');
            }

            const summaries = getSummaries();
            summaries.unshift({ url: url, summary: data.summary, transcript: data.subtitles });
            saveSummaries(summaries);
            activeSummaryIndex = 0;
            renderSavedSummaries();

        } catch (error) {
            setStatus(false, error.message);
        }
    }


    loadSettings();
    renderSavedSummaries();
    form.addEventListener('submit', summarize);
    clearSummariesBtn.addEventListener('click', clearSummaries);

    newSummaryBtn.addEventListener('click', () => {
        activeSummaryIndex = -1;
        urlInput.value = '';
        renderSavedSummaries();
        switchView(false);
    });

    savedSummariesList.addEventListener('click', (e) => {
        const link = e.target.closest('a');
        const deleteBtn = e.target.closest('.delete-summary-btn');

        if (deleteBtn) {
            e.preventDefault();
            const index = parseInt(deleteBtn.dataset.index, 10);
            if (!isNaN(index)) {
                deleteSummary(index);
            }
            return;
        }

        if (link && link.dataset.index) {
            e.preventDefault();
            const index = parseInt(link.dataset.index, 10);
            if (!isNaN(index)) {
                viewSummary(index);
            }
        }
    });
});

window.addEventListener('unhandledrejection', event => {
    console.error('Unhandled rejection:', event.reason);
});
window.addEventListener('error', event => {
    console.error('Uncaught error:', event.error);
});
