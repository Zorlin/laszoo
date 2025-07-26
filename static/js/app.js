function laszooApp() {
    return {
        // State
        activeTab: 'overview',
        hostname: 'Loading...',
        mfsStatus: 'unknown',
        serviceStatus: 'unknown',
        serviceMode: 'watch',
        gamepadConnected: false,
        gamepadName: '',
        
        // Files
        files: [],
        fileSearch: '',
        
        // Groups
        groups: [],
        
        // Operations
        activeOperations: [],
        recentActivity: [],
        
        // Modals
        showEnrollModal: false,
        showCreateGroupModal: false,
        
        // Forms
        enrollForm: {
            path: '',
            group: '',
            action: 'converge',
            machineSpecific: false
        },
        
        // Gamepad
        gamepadMappings: {
            a: 'sync',
            b: 'cancel',
            dpad: 'navigation',
            leftStick: 'scroll'
        },
        leftStickX: 0,
        leftStickY: 0,
        rightStickX: 0,
        rightStickY: 0,
        activeButtons: [],
        
        // WebSocket
        ws: null,
        
        // Computed
        get filteredFiles() {
            if (!this.fileSearch) return this.files;
            const search = this.fileSearch.toLowerCase();
            return this.files.filter(file => 
                file.path.toLowerCase().includes(search) ||
                file.group.toLowerCase().includes(search)
            );
        },
        
        // Methods
        async init() {
            await this.loadStatus();
            await this.loadFiles();
            await this.loadGroups();
            this.connectWebSocket();
            this.startGamepadPolling();
            
            // Refresh status every 5 seconds
            setInterval(() => this.loadStatus(), 5000);
        },
        
        async loadStatus() {
            try {
                const response = await fetch('/api/status');
                const data = await response.json();
                this.hostname = data.hostname;
                this.mfsStatus = data.mfs_mounted ? 'connected' : 'disconnected';
                this.serviceStatus = data.service_status;
                this.serviceMode = data.service_mode || 'watch';
            } catch (error) {
                console.error('Failed to load status:', error);
            }
        },
        
        async loadFiles() {
            try {
                const response = await fetch('/api/files');
                const data = await response.json();
                this.files = data.files || [];
            } catch (error) {
                console.error('Failed to load files:', error);
            }
        },
        
        async loadGroups() {
            try {
                const response = await fetch('/api/groups');
                const data = await response.json();
                this.groups = data.groups || [];
            } catch (error) {
                console.error('Failed to load groups:', error);
            }
        },
        
        connectWebSocket() {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${protocol}//${window.location.host}/ws`;
            
            this.ws = new WebSocket(wsUrl);
            
            this.ws.onopen = () => {
                console.log('WebSocket connected');
                this.addActivity('WebSocket connected');
            };
            
            this.ws.onmessage = (event) => {
                const message = JSON.parse(event.data);
                this.handleWebSocketMessage(message);
            };
            
            this.ws.onclose = () => {
                console.log('WebSocket disconnected, reconnecting...');
                this.addActivity('WebSocket disconnected');
                setTimeout(() => this.connectWebSocket(), 5000);
            };
            
            this.ws.onerror = (error) => {
                console.error('WebSocket error:', error);
            };
        },
        
        handleWebSocketMessage(message) {
            switch (message.type) {
                case 'file_changed':
                    this.handleFileChanged(message.data);
                    break;
                case 'status_update':
                    this.handleStatusUpdate(message.data);
                    break;
                case 'operation_update':
                    this.handleOperationUpdate(message.data);
                    break;
                case 'gamepad_event':
                    this.handleGamepadEvent(message.data);
                    break;
            }
        },
        
        handleFileChanged(data) {
            const fileIndex = this.files.findIndex(f => f.path === data.path);
            if (fileIndex >= 0) {
                this.files[fileIndex] = { ...this.files[fileIndex], ...data };
            } else {
                this.files.push(data);
            }
            this.addActivity(`File ${data.path} ${data.status}`);
        },
        
        handleStatusUpdate(data) {
            Object.assign(this, data);
        },
        
        handleOperationUpdate(data) {
            const opIndex = this.activeOperations.findIndex(op => op.id === data.id);
            if (opIndex >= 0) {
                if (data.status === 'completed' || data.status === 'failed') {
                    this.activeOperations.splice(opIndex, 1);
                    this.addActivity(`Operation ${data.type} ${data.status}`);
                } else {
                    this.activeOperations[opIndex] = data;
                }
            } else if (data.status === 'running') {
                this.activeOperations.push(data);
            }
        },
        
        handleGamepadEvent(data) {
            // Handle gamepad events from server
            if (data.action && this.gamepadMappings[data.button] !== 'none') {
                this.executeGamepadAction(this.gamepadMappings[data.button]);
            }
        },
        
        addActivity(message) {
            const now = new Date();
            const time = now.toLocaleTimeString();
            this.recentActivity.unshift({
                id: Date.now(),
                time,
                message
            });
            // Keep only last 20 activities
            if (this.recentActivity.length > 20) {
                this.recentActivity.pop();
            }
        },
        
        async syncAll() {
            try {
                const response = await fetch('/api/sync/all', { method: 'POST' });
                const data = await response.json();
                this.addActivity('Started sync all operation');
            } catch (error) {
                console.error('Failed to sync all:', error);
                this.addActivity('Failed to start sync all');
            }
        },
        
        async checkStatus() {
            try {
                const response = await fetch('/api/status/check', { method: 'POST' });
                const data = await response.json();
                this.addActivity('Status check completed');
                await this.loadFiles();
            } catch (error) {
                console.error('Failed to check status:', error);
                this.addActivity('Failed to check status');
            }
        },
        
        async reloadService() {
            try {
                const response = await fetch('/api/service/reload', { method: 'POST' });
                const data = await response.json();
                this.addActivity('Service reload requested');
            } catch (error) {
                console.error('Failed to reload service:', error);
                this.addActivity('Failed to reload service');
            }
        },
        
        async enrollFile() {
            try {
                const response = await fetch('/api/files/enroll', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(this.enrollForm)
                });
                
                if (response.ok) {
                    this.showEnrollModal = false;
                    this.addActivity(`Enrolled ${this.enrollForm.path}`);
                    await this.loadFiles();
                    this.enrollForm = {
                        path: '',
                        group: '',
                        action: 'converge',
                        machineSpecific: false
                    };
                } else {
                    const error = await response.json();
                    alert(`Failed to enroll file: ${error.message}`);
                }
            } catch (error) {
                console.error('Failed to enroll file:', error);
                alert('Failed to enroll file');
            }
        },
        
        async syncFile(file) {
            try {
                const response = await fetch(`/api/files/${encodeURIComponent(file.path)}/sync`, {
                    method: 'POST'
                });
                
                if (response.ok) {
                    this.addActivity(`Syncing ${file.path}`);
                } else {
                    const error = await response.json();
                    alert(`Failed to sync file: ${error.message}`);
                }
            } catch (error) {
                console.error('Failed to sync file:', error);
                alert('Failed to sync file');
            }
        },
        
        async unenrollFile(file) {
            if (!confirm(`Are you sure you want to unenroll ${file.path}?`)) {
                return;
            }
            
            try {
                const response = await fetch(`/api/files/${encodeURIComponent(file.path)}`, {
                    method: 'DELETE'
                });
                
                if (response.ok) {
                    this.addActivity(`Unenrolled ${file.path}`);
                    await this.loadFiles();
                } else {
                    const error = await response.json();
                    alert(`Failed to unenroll file: ${error.message}`);
                }
            } catch (error) {
                console.error('Failed to unenroll file:', error);
                alert('Failed to unenroll file');
            }
        },
        
        async syncGroup(group) {
            try {
                const response = await fetch(`/api/groups/${group.name}/sync`, {
                    method: 'POST'
                });
                
                if (response.ok) {
                    this.addActivity(`Syncing group ${group.name}`);
                } else {
                    const error = await response.json();
                    alert(`Failed to sync group: ${error.message}`);
                }
            } catch (error) {
                console.error('Failed to sync group:', error);
                alert('Failed to sync group');
            }
        },
        
        viewGroup(group) {
            this.fileSearch = `group:${group.name}`;
            this.activeTab = 'files';
        },
        
        startGamepadPolling() {
            // Gamepad polling is handled by gamepad.js
            // This just updates the UI state
            window.addEventListener('gamepadconnected', (e) => {
                this.gamepadConnected = true;
                this.gamepadName = e.gamepad.id;
                this.addActivity(`Gamepad connected: ${e.gamepad.id}`);
            });
            
            window.addEventListener('gamepaddisconnected', (e) => {
                this.gamepadConnected = false;
                this.gamepadName = '';
                this.addActivity('Gamepad disconnected');
            });
        },
        
        executeGamepadAction(action) {
            switch (action) {
                case 'sync':
                    if (this.activeTab === 'files' && this.filteredFiles.length > 0) {
                        this.syncFile(this.filteredFiles[0]);
                    }
                    break;
                case 'apply':
                    this.syncAll();
                    break;
                case 'status':
                    this.checkStatus();
                    break;
                case 'cancel':
                    // Cancel active operation if any
                    if (this.activeOperations.length > 0) {
                        // TODO: Implement cancel operation
                    }
                    break;
                case 'back':
                    this.activeTab = 'overview';
                    break;
            }
        }
    };
}