use async_trait::async_trait;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{window, IdbDatabase, IdbRequest, IdbTransactionMode};

use crate::data_storage::{DataStorage, StorageError};

pub struct DataStorageLocal {
    base: String,
}

impl DataStorageLocal {
    pub fn new(base_str: &str) -> Box<dyn DataStorage + Send> {
        let base = if base_str.is_empty() {
            "".to_string()
        } else {
            base_str.to_string() + "_"
        };
        Box::new(DataStorageLocal { base })
    }
}

#[async_trait(?Send)]
impl DataStorage for DataStorageLocal {
    fn get(&self, key: &str) -> Result<String, StorageError> {
        let key_entry = format!("{}{}", self.base, key);
        Ok(window()
            .unwrap()
            .local_storage()
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))?
            .unwrap()
            .get(&key_entry)
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))?
            .unwrap_or_else(|| "".to_string()))
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        let key_entry = format!("{}{}", self.base, key);
        window()
            .unwrap()
            .local_storage()
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))?
            .unwrap()
            .set(&key_entry, value)
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))
    }

    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        let key_entry = format!("{}{}", self.base, key);
        window()
            .unwrap()
            .local_storage()
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))?
            .unwrap()
            .remove_item(&key_entry)
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))
    }

    fn clone_box(&self) -> Box<dyn DataStorage + Send> {
        Box::new(DataStorageLocal {
            base: self.base.clone(),
        })
    }
}

const STORE_NAME: &str = "kv_store";

/// IndexedDB-based data storage that works in both window and shared worker contexts.
///
/// Since IndexedDB operations are asynchronous but the DataStorage trait is synchronous,
/// this implementation caches all data in memory. The async `new` constructor loads
/// existing data from IndexedDB, and all mutations are persisted asynchronously.
pub struct DataStorageIndexedDB {
    db_name: String,
    cache: Arc<Mutex<HashMap<String, String>>>,
    db: Arc<Mutex<Option<IdbDatabase>>>,
    future_write: Arc<Mutex<HashMap<String, String>>>,
}

unsafe impl Send for DataStorageIndexedDB {}

impl DataStorageIndexedDB {
    /// Creates a new IndexedDB-backed storage.
    ///
    /// This async constructor opens the database, creates the object store if needed,
    /// and loads all existing key-value pairs into memory.
    pub async fn new(db_name: &str) -> Result<Box<dyn DataStorage + Send>, StorageError> {
        let db = Self::open_database(db_name).await?;
        let cache = Self::load_all_data(&db).await?;

        Ok(Box::new(DataStorageIndexedDB {
            db_name: db_name.to_string(),
            cache: Arc::new(Mutex::new(cache)),
            db: Arc::new(Mutex::new(Some(db))),
            future_write: Arc::new(Mutex::new(HashMap::new())),
        }))
    }

    /// Gets the IDBFactory from either window or worker global scope.
    fn get_idb_factory() -> Result<web_sys::IdbFactory, StorageError> {
        // Try window first (regular browser context)
        if let Some(window) = web_sys::window() {
            if let Ok(Some(idb)) = window.indexed_db() {
                return Ok(idb);
            }
        }

        // Fall back to WorkerGlobalScope (shared worker context)
        let global = js_sys::global();
        let idb = js_sys::Reflect::get(&global, &JsValue::from_str("indexedDB"))
            .map_err(|e| StorageError::Underlying(format!("Failed to get indexedDB: {:?}", e)))?;

        if idb.is_undefined() || idb.is_null() {
            return Err(StorageError::Underlying(
                "IndexedDB not available in this context".to_string(),
            ));
        }

        idb.dyn_into::<web_sys::IdbFactory>()
            .map_err(|_| StorageError::Underlying("Failed to cast to IdbFactory".to_string()))
    }

    async fn open_database(db_name: &str) -> Result<IdbDatabase, StorageError> {
        let idb_factory = Self::get_idb_factory()?;

        let open_request = idb_factory
            .open_with_u32(db_name, 1)
            .map_err(|e| StorageError::Underlying(format!("Failed to open database: {:?}", e)))?;

        // Set up the upgrade handler to create the object store
        let store_name = STORE_NAME.to_string();
        let onupgradeneeded = Closure::once(move |event: web_sys::IdbVersionChangeEvent| {
            let target = event.target().unwrap();
            let request: IdbRequest = target.dyn_into().unwrap();
            let db: IdbDatabase = request.result().unwrap().dyn_into().unwrap();

            if !db.object_store_names().contains(&store_name) {
                db.create_object_store(&store_name)
                    .expect("Failed to create object store");
            }
        });
        open_request.set_onupgradeneeded(Some(onupgradeneeded.as_ref().unchecked_ref()));
        onupgradeneeded.forget();

        // Wait for the database to open
        let result = Self::await_request(&open_request).await?;

        result.dyn_into::<IdbDatabase>().map_err(|_| {
            StorageError::Underlying("Failed to cast result to IdbDatabase".to_string())
        })
    }

    async fn load_all_data(db: &IdbDatabase) -> Result<HashMap<String, String>, StorageError> {
        let transaction = db
            .transaction_with_str_and_mode(STORE_NAME, IdbTransactionMode::Readonly)
            .map_err(|e| {
                StorageError::Underlying(format!("Failed to create transaction: {:?}", e))
            })?;

        let store = transaction.object_store(STORE_NAME).map_err(|e| {
            StorageError::Underlying(format!("Failed to get object store: {:?}", e))
        })?;

        let request = store
            .open_cursor()
            .map_err(|e| StorageError::Underlying(format!("Failed to open cursor: {:?}", e)))?;

        let data = Arc::new(Mutex::new(HashMap::new()));
        let data_clone = Arc::clone(&data);

        // Use a promise to collect all cursor results
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            let data_inner = Arc::clone(&data_clone);
            let resolve_clone = resolve.clone();

            let onsuccess = Closure::wrap(Box::new(move |event: web_sys::Event| {
                let target = event.target().unwrap();
                let request: IdbRequest = target.dyn_into().unwrap();
                let result = request.result().unwrap();

                if result.is_null() || result.is_undefined() {
                    // No more entries
                    resolve_clone.call0(&JsValue::NULL).unwrap();
                } else {
                    let cursor: web_sys::IdbCursorWithValue = result.dyn_into().unwrap();
                    let key = cursor.key().unwrap();
                    let value = cursor.value().unwrap();

                    if let (Some(k), Some(v)) = (key.as_string(), value.as_string()) {
                        log::info!("lock 192");
                        let mut cache = data_inner.lock().unwrap();
                        log::info!("lock 194");
                        cache.insert(k, v);
                    }

                    cursor.continue_().unwrap();
                }
            }) as Box<dyn FnMut(_)>);

            request.set_onsuccess(Some(onsuccess.as_ref().unchecked_ref()));
            onsuccess.forget();
        });

        JsFuture::from(promise)
            .await
            .map_err(|e| StorageError::Underlying(format!("Failed to load data: {:?}", e)))?;

        log::info!("lock 210");
        let result = data.lock().unwrap().clone();
        log::info!("lock 212");
        Ok(result)
    }

    async fn await_request(request: &IdbRequest) -> Result<JsValue, StorageError> {
        let promise = js_sys::Promise::new(&mut |resolve, reject| {
            let resolve_clone = resolve.clone();
            let reject_clone = reject.clone();

            let onsuccess = Closure::once(move |_event: web_sys::Event| {
                resolve_clone.call0(&JsValue::NULL).unwrap();
            });

            let onerror = Closure::once(move |_event: web_sys::Event| {
                reject_clone
                    .call1(&JsValue::NULL, &JsValue::from_str("Request failed"))
                    .unwrap();
            });

            request.set_onsuccess(Some(onsuccess.as_ref().unchecked_ref()));
            request.set_onerror(Some(onerror.as_ref().unchecked_ref()));

            onsuccess.forget();
            onerror.forget();
        });

        JsFuture::from(promise)
            .await
            .map_err(|e| StorageError::Underlying(format!("Request failed: {:?}", e)))?;

        request
            .result()
            .map_err(|e| StorageError::Underlying(format!("Failed to get result: {:?}", e)))
    }

    /// Persists a key-value pair to IndexedDB asynchronously.
    fn persist_set(&self, key: String, value: String) {
        let db_arc = Arc::clone(&self.db);
        let future_write = Arc::clone(&self.future_write);

        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = Self::do_persist_set(&db_arc, &future_write, &key, &value).await {
                log::error!("Failed to persist to IndexedDB: {:?}", e);
            }
        });
    }

    async fn do_persist_set(
        db_arc: &Arc<Mutex<Option<IdbDatabase>>>,
        future_write: &Arc<Mutex<HashMap<String, String>>>,
        key: &str,
        value: &str,
    ) -> Result<(), StorageError> {
        log::info!("lock 263");
        future_write
            .lock()
            .unwrap()
            .insert(key.into(), value.into());
        let db = match db_arc.try_lock() {
            Ok(db) => db,
            Err(_) => return Ok(()),
        };
        log::info!("lock 265");
        let kvs = future_write.lock().unwrap().drain().collect::<Vec<_>>();
        for kv in kvs {
            log::info!("Storing {kv:?} persistently");
            let db = db
                .as_ref()
                .ok_or_else(|| StorageError::Underlying("Database not initialized".to_string()))?;

            let transaction = db
                .transaction_with_str_and_mode(STORE_NAME, IdbTransactionMode::Readwrite)
                .map_err(|e| {
                    StorageError::Underlying(format!("Failed to create transaction: {:?}", e))
                })?;

            let store = transaction.object_store(STORE_NAME).map_err(|e| {
                StorageError::Underlying(format!("Failed to get object store: {:?}", e))
            })?;

            let request = store
                .put_with_key(&JsValue::from_str(&kv.1), &JsValue::from_str(&kv.0))
                .map_err(|e| StorageError::Underlying(format!("Failed to put value: {:?}", e)))?;

            log::info!("await_request start");
            Self::await_request(&request).await?;
            log::info!("await_request end");
        }
        Ok(())
    }

    /// Removes a key from IndexedDB asynchronously.
    fn persist_remove(&self, key: String) {
        let db_arc = Arc::clone(&self.db);

        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = Self::do_persist_remove(&db_arc, &key).await {
                log::error!("Failed to remove from IndexedDB: {:?}", e);
            }
        });
    }

    async fn do_persist_remove(
        db_arc: &Arc<Mutex<Option<IdbDatabase>>>,
        key: &str,
    ) -> Result<(), StorageError> {
        log::info!("lock 303");
        let db = db_arc.lock().unwrap();
        log::info!("lock 305");
        let db = db
            .as_ref()
            .ok_or_else(|| StorageError::Underlying("Database not initialized".to_string()))?;

        let transaction = db
            .transaction_with_str_and_mode(STORE_NAME, IdbTransactionMode::Readwrite)
            .map_err(|e| {
                StorageError::Underlying(format!("Failed to create transaction: {:?}", e))
            })?;

        let store = transaction.object_store(STORE_NAME).map_err(|e| {
            StorageError::Underlying(format!("Failed to get object store: {:?}", e))
        })?;

        let request = store
            .delete(&JsValue::from_str(key))
            .map_err(|e| StorageError::Underlying(format!("Failed to delete key: {:?}", e)))?;

        Self::await_request(&request).await?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl DataStorage for DataStorageIndexedDB {
    fn get(&self, key: &str) -> Result<String, StorageError> {
        log::info!("lock 332");
        let cache = self
            .cache
            .lock()
            .map_err(|e| StorageError::Underlying(e.to_string()))?;
        log::info!("lock 337");
        Ok(cache.get(key).cloned().unwrap_or_default())
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        {
            log::info!("lock 343");
            let mut cache = self
                .cache
                .lock()
                .map_err(|e| StorageError::Underlying(e.to_string()))?;
            log::info!("lock 348");
            cache.insert(key.to_string(), value.to_string());
        }

        // Persist to IndexedDB asynchronously
        self.persist_set(key.to_string(), value.to_string());
        Ok(())
    }

    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        {
            log::info!("lock 359");
            let mut cache = self
                .cache
                .lock()
                .map_err(|e| StorageError::Underlying(e.to_string()))?;
            log::info!("lock 364");
            cache.remove(key);
        }

        // Remove from IndexedDB asynchronously
        self.persist_remove(key.to_string());
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn DataStorage + Send> {
        Box::new(DataStorageIndexedDB {
            db_name: self.db_name.clone(),
            cache: Arc::clone(&self.cache),
            db: Arc::clone(&self.db),
            future_write: Arc::clone(&self.future_write),
        })
    }
}
