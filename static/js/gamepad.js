// Gamepad support for Laszoo
(function() {
    let controllers = {};
    let animationFrame = null;
    let ws = null;
    
    // Button mapping
    const BUTTON_NAMES = [
        'a', 'b', 'x', 'y',
        'lb', 'rb', 'lt', 'rt',
        'back', 'start',
        'ls', 'rs',
        'up', 'down', 'left', 'right',
        'home'
    ];
    
    function connectWebSocket() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        ws = new WebSocket(`${protocol}//${window.location.host}/ws/gamepad`);
        
        ws.onopen = () => {
            console.log('Gamepad WebSocket connected');
        };
        
        ws.onclose = () => {
            console.log('Gamepad WebSocket disconnected, reconnecting...');
            setTimeout(connectWebSocket, 5000);
        };
        
        ws.onerror = (error) => {
            console.error('Gamepad WebSocket error:', error);
        };
    }
    
    function sendGamepadEvent(type, data) {
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({
                type: 'gamepad_input',
                data: {
                    type: type,
                    ...data
                }
            }));
        }
    }
    
    window.addEventListener('gamepadconnected', (e) => {
        const gamepad = e.gamepad;
        controllers[gamepad.index] = {
            gamepad: gamepad,
            prevButtons: new Array(gamepad.buttons.length).fill(false),
            prevAxes: [...gamepad.axes]
        };
        
        console.log(`Gamepad connected: ${gamepad.id} (index: ${gamepad.index})`);
        
        if (!animationFrame) {
            updateGamepads();
        }
    });
    
    window.addEventListener('gamepaddisconnected', (e) => {
        delete controllers[e.gamepad.index];
        console.log(`Gamepad disconnected: ${e.gamepad.id}`);
        
        if (Object.keys(controllers).length === 0 && animationFrame) {
            cancelAnimationFrame(animationFrame);
            animationFrame = null;
        }
    });
    
    function updateGamepads() {
        // Get fresh gamepad states
        const gamepads = navigator.getGamepads();
        
        for (let i = 0; i < gamepads.length; i++) {
            const gamepad = gamepads[i];
            if (!gamepad || !controllers[i]) continue;
            
            const controller = controllers[i];
            controller.gamepad = gamepad;
            
            // Check buttons
            for (let j = 0; j < gamepad.buttons.length; j++) {
                const button = gamepad.buttons[j];
                const pressed = button.pressed || button.value > 0.5;
                
                if (pressed !== controller.prevButtons[j]) {
                    controller.prevButtons[j] = pressed;
                    
                    const buttonName = BUTTON_NAMES[j] || `button${j}`;
                    sendGamepadEvent(pressed ? 'button_down' : 'button_up', {
                        controller: i,
                        button: buttonName,
                        value: button.value
                    });
                    
                    // Update UI if app exists
                    if (window.app) {
                        if (pressed) {
                            if (!window.app.activeButtons.includes(buttonName)) {
                                window.app.activeButtons.push(buttonName);
                            }
                        } else {
                            const idx = window.app.activeButtons.indexOf(buttonName);
                            if (idx >= 0) {
                                window.app.activeButtons.splice(idx, 1);
                            }
                        }
                    }
                }
            }
            
            // Check axes (analog sticks and triggers)
            for (let j = 0; j < gamepad.axes.length; j++) {
                const axis = gamepad.axes[j];
                const prevAxis = controller.prevAxes[j];
                
                // Only send updates if change is significant (deadzone)
                if (Math.abs(axis - prevAxis) > 0.01) {
                    controller.prevAxes[j] = axis;
                    
                    let axisName;
                    switch (j) {
                        case 0: axisName = 'left_x'; break;
                        case 1: axisName = 'left_y'; break;
                        case 2: axisName = 'right_x'; break;
                        case 3: axisName = 'right_y'; break;
                        default: axisName = `axis${j}`; break;
                    }
                    
                    sendGamepadEvent('axis_move', {
                        controller: i,
                        axis: axisName,
                        value: axis
                    });
                    
                    // Update UI if app exists
                    if (window.app) {
                        switch (j) {
                            case 0: window.app.leftStickX = axis; break;
                            case 1: window.app.leftStickY = axis; break;
                            case 2: window.app.rightStickX = axis; break;
                            case 3: window.app.rightStickY = axis; break;
                        }
                    }
                }
            }
            
            // Handle D-pad as axes on some controllers
            if (gamepad.axes.length >= 7) {
                const dpadX = gamepad.axes[6];
                const dpadY = gamepad.axes[7];
                
                if (dpadX !== controller.prevAxes[6] || dpadY !== controller.prevAxes[7]) {
                    controller.prevAxes[6] = dpadX;
                    controller.prevAxes[7] = dpadY;
                    
                    let direction = null;
                    if (dpadX < -0.5) direction = 'left';
                    else if (dpadX > 0.5) direction = 'right';
                    else if (dpadY < -0.5) direction = 'up';
                    else if (dpadY > 0.5) direction = 'down';
                    
                    if (direction) {
                        sendGamepadEvent('dpad', {
                            controller: i,
                            direction: direction
                        });
                    }
                }
            }
        }
        
        animationFrame = requestAnimationFrame(updateGamepads);
    }
    
    // Gamepad API helper functions
    window.gamepadSupport = {
        getControllers: () => controllers,
        
        vibrate: (controllerIndex, duration = 200, weakMagnitude = 0.5, strongMagnitude = 1.0) => {
            const controller = controllers[controllerIndex];
            if (!controller || !controller.gamepad.vibrationActuator) return;
            
            controller.gamepad.vibrationActuator.playEffect('dual-rumble', {
                startDelay: 0,
                duration: duration,
                weakMagnitude: weakMagnitude,
                strongMagnitude: strongMagnitude
            });
        },
        
        isButtonPressed: (controllerIndex, buttonName) => {
            const controller = controllers[controllerIndex];
            if (!controller) return false;
            
            const buttonIndex = BUTTON_NAMES.indexOf(buttonName);
            if (buttonIndex < 0) return false;
            
            const button = controller.gamepad.buttons[buttonIndex];
            return button && (button.pressed || button.value > 0.5);
        },
        
        getAxis: (controllerIndex, axisName) => {
            const controller = controllers[controllerIndex];
            if (!controller) return 0;
            
            const axisMap = {
                'left_x': 0,
                'left_y': 1,
                'right_x': 2,
                'right_y': 3
            };
            
            const axisIndex = axisMap[axisName];
            if (axisIndex === undefined) return 0;
            
            return controller.gamepad.axes[axisIndex] || 0;
        }
    };
    
    // Initialize WebSocket connection
    connectWebSocket();
    
    // Expose to Alpine.js app when it's ready
    document.addEventListener('alpine:init', () => {
        Alpine.data('gamepad', () => ({
            controllers: controllers
        }));
    });
})();