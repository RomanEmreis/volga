let count = 0;

const increaseCount = () => {
    count++;
    document.getElementById("count").textContent = count.toString();
};

const decreaseCount = () => {
    count--;
    document.getElementById("count").textContent = count.toString();
};
