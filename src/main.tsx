import ReactDOM from "react-dom/client";
import App from "./App";
import { initAndroid } from "./lib/android";
import "./App.css";

initAndroid();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <App />
);
