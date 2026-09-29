import { mount } from 'svelte'
import App from './App.svelte'
import './app.css'
import { followTheSystemBars } from './lib/insets'

followTheSystemBars()

mount(App, { target: document.getElementById('app')! })
