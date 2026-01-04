//https://medium.com/@erlan.zharkeev/how-to-detect-when-a-computer-wakes-up-from-sleep-my-experience-solving-the-problem-with-6639f79e5275

const CHECK_INTERVAL = 60*1000;
const SLEEP_THRESHOLD = 30*60*1000;
let lastCheckTime = Date.now();

const checkIfComputerSlept = () => {
	const currentTime = Date.now();
	const timeDifference = currentTime - lastCheckTime;

	if (timeDifference > SLEEP_THRESHOLD) {
		postMessage('computer-slept');
	}
	lastCheckTime = currentTime;
	setTimeout(checkIfComputerSlept, CHECK_INTERVAL);
};

checkIfComputerSlept();