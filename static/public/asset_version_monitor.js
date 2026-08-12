(() => {
	const API_PATH = '/api';
	const HASH_HEADER = 'x-webhome-wasm-hash';
	const LAST_HASH_STORAGE_KEY = 'webhome.asset-version.last-hash';
	const RELOAD_TARGET_STORAGE_KEY = 'webhome.asset-version.reload-target';
	const INSTALLATION_FLAG = '__webhomeAssetVersionMonitorInstalled';

	if (window[INSTALLATION_FLAG]) {
		return;
	}
	window[INSTALLATION_FLAG] = true;

	const storageGet = (key) => {
		try {
			return window.sessionStorage.getItem(key);
		} catch (_) {
			return null;
		}
	};
	const storageSet = (key, value) => {
		try {
			window.sessionStorage.setItem(key, value);
			return true;
		} catch (_) {
			return false;
		}
	};
	const storageRemove = (key) => {
		try {
			window.sessionStorage.removeItem(key);
		} catch (_) {
		}
	};
	const hashIsValid = (hash) => typeof hash === 'string'
		&& /^[A-Za-z0-9_-]{1,128}$/.test(hash);
	const clientWasmHashGet = () => {
		const preload = document.querySelector('link[rel="preload"][as="fetch"][type="application/wasm"]');
		if (!preload) {
			return null;
		}
		const filename = new URL(preload.href,document.baseURI).pathname.split('/').pop() || '';
		return filename.match(/\.([A-Za-z0-9_-]{1,128})\.wasm$/)?.[1] || null;
	};
	const responseIsApi = (response) => {
		const url = new URL(response.url,document.baseURI);
		return url.origin === window.location.origin
			&& (url.pathname === API_PATH || url.pathname.startsWith(`${API_PATH}/`));
	};
	const responseVersionApply = (response) => {
		if (!responseIsApi(response)) {
			return;
		}
		const serverHash = response.headers.get(HASH_HEADER);
		if (serverHash === null) {
			return;
		}
		if (!hashIsValid(serverHash)) {
			console.warn('WebHome ignored an invalid WASM hash response header.');
			return;
		}

		const previousServerHash = storageGet(LAST_HASH_STORAGE_KEY);
		const clientHash = clientWasmHashGet();
		storageSet(LAST_HASH_STORAGE_KEY,serverHash);
		const releaseChanged = clientHash !== null
			? clientHash !== serverHash
			: hashIsValid(previousServerHash) && previousServerHash !== serverHash;
		if (!releaseChanged) {
			if (clientHash === serverHash) {
				storageRemove(RELOAD_TARGET_STORAGE_KEY);
			}
			return;
		}

		if (storageGet(RELOAD_TARGET_STORAGE_KEY) === serverHash) {
			console.error('WebHome detected a newer WASM bundle, but the previous reload did not activate it.');
			return;
		}
		if (!storageSet(RELOAD_TARGET_STORAGE_KEY,serverHash)) {
			console.error('WebHome cannot safely reload the newer WASM bundle because session storage is unavailable.');
			return;
		}
		window.location.reload();
	};

	const fetchOriginal = window.fetch.bind(window);
	window.fetch = async (...args) => {
		const response = await fetchOriginal(...args);
		try {
			responseVersionApply(response);
		} catch (error) {
			console.error('WebHome could not inspect the API asset-version header.',error);
		}
		return response;
	};
})();
