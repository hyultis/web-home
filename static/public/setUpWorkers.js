//https://medium.com/@erlan.zharkeev/how-to-detect-when-a-computer-wakes-up-from-sleep-my-experience-solving-the-problem-with-6639f79e5275

const PAGE_RELOAD_DELAY_IN_SEC = 3;

export const useSleepWorker = () => {
	if (window.Worker) {
		const sleepWorker = new Worker('./sleep-worker.js', { type: 'module' });
		sleepWorker.onmessage = function (e) {
			if (e.data === 'computer-slept') {
				console.log(`The computer likely went to sleep and woke up later. Reloading the page in ${PAGE_RELOAD_DELAY_IN_SEC} seconds.`);
				setTimeout(() => {
					window.location.reload();
				}, PAGE_RELOAD_DELAY_IN_SEC * 1000);
			}
		};
		console.log('Sleep worker started.');
	}
};

console.log('Sleep worker autostart ?');
useSleepWorker();