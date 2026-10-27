// Entry point: mount the app inside the session provider.

import { render } from 'solid-js/web';
import { SessionProvider } from './state/session.jsx';
import App from './App.jsx';
import './styles/tokens.css';
import './styles/app.css';

const root = document.getElementById('root');
render(
  () => (
    <SessionProvider>
      <App />
    </SessionProvider>
  ),
  root,
);
