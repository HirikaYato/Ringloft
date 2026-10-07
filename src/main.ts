import { mount } from 'svelte';

// Встроенные шрифты для выбора в настройках. В @font-face у них
// unicode-range, так что подгружаются только нужные наборы символов.
import '@fontsource-variable/golos-text';
import '@fontsource-variable/inter';
import '@fontsource-variable/manrope';
import '@fontsource-variable/nunito';
import './styles/tokens.css';
import './styles/themes/dark.css';
import './styles/themes/light.css';
import { api } from './lib/api';

const target = document.getElementById('app');
if (!target) throw new Error('#app не найден в index.html');

// У телефона свой корневой макет на тех же сторах. Какой нужен, знает
// бэкенд; не ответил — берём десктопный, он покажет ошибку запуска сам.
// Макеты грузятся по требованию: чужой в память не попадает.
const mobile = await api
  .appInfo()
  .then((info) => info.mobile)
  .catch(() => false);
const root = mobile ? await import('./MobileApp.svelte') : await import('./App.svelte');

export default mount(root.default, { target });
