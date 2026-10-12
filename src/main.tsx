import ReactDOM from "react-dom/client";
import App from "./App";
import { initAndroid } from "./lib/android";
import { initPlatform } from "./lib/platform";
import "./App.css";

initAndroid();
initPlatform();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <App />
);
