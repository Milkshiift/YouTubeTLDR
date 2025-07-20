document.addEventListener('DOMContentLoaded', () => {
    // --- DOM Elements ---
    const form = document.getElementById('summary-form');
    const urlInput = document.getElementById('youtube-url');
    const apiKeyInput = document.getElementById('api-key');
    const modelInput = document.getElementById('model');
    const systemPromptInput = document.getElementById('system-prompt');
    const dryRunCheckbox = document.getElementById('dry-run');

    const statusContainer = document.getElementById('status-container');
    const loader = document.getElementById('loader');
    const errorMessage = document.getElementById('error-message');

    const summaryContainer = document.getElementById('summary-container');
    const summaryOutput = document.getElementById('summary-output');

    const transcriptSection = document.getElementById('transcript-section');
    const transcriptText = document.getElementById('transcript-text');

    const baseURL = `${location.protocol}//${location.hostname}${location.port ? ':' + location.port : ''}`;

    // --- Local Storage Keys ---
    const API_KEY_STORAGE_KEY = 'youtube-tldr-api-key';
    const MODEL_STORAGE_KEY = 'youtube-tldr-model';
    const SYSTEM_PROMPT_STORAGE_KEY = 'youtube-tldr-system-prompt';
    const DRY_RUN_STORAGE_KEY = 'youtube-tldr-dry-run';

    const DEFAULT_SYSTEM_PROMPT = `You are an expert video summarizer. Create a structured, accurate overview of the provided transcript. Use clear headings for topics and bullet points for key details. Bold essential terms and concepts. Focus on the most important information, removing conversational filler.`;

    // --- Functions ---
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

        loader.style.display = isLoading ? 'block' : 'none';
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
            summaryOutput.mdContent = data.summary;
            summaryContainer.classList.remove('hidden');

            if (data.subtitles && data.subtitles.trim()) {
                transcriptText.textContent = data.subtitles;
                transcriptSection.classList.remove('hidden');
            }

        } catch (error) {
            setStatus(false, error.message);
        }
    }

    // --- Initial Load & Event Listeners ---
    loadSettings();
    form.addEventListener('submit', summarize);
});