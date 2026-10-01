import { mount } from 'svelte';

import App from './App.svelte';
// Встроенные шрифты для выбора в настройках. В @font-face у них
// unicode-range, так что подгружаются только нужные наборы символов.
import '@fontsource-variable/golos-text';
import '@fontsource-variable/inter';
import '@fontsource-variable/manrope';
import '@fontsource-variable/nunito';
import './styles/tokens.css';
import './styles/themes/dark.css';
import './styles/themes/light.css';

const target = document.getElementById('app');
if (!target) throw new Error('#app не найден в index.html');

export default mount(App, { target });
