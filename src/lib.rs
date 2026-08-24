use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]

pub enum OperationStatus {
    Running,
    Success,
    Failed,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
pub struct OperationId(u64);

pub struct Operation {
    pub id: OperationId,
    pub name: String,
    pub parent_id: Option<OperationId>,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub status: OperationStatus,
    pub failure_reason: Option<String>,
}

impl Operation {
    pub fn new(id: OperationId, name: String, parent_id: Option<OperationId>) -> Self {
        Self {
            id,
            name,
            parent_id,
            start_time: Instant::now(),
            end_time: None,
            status: OperationStatus::Running,
            failure_reason: None,
        }
    }
}

pub fn extract_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        return s.to_string();
    } else if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    } else if let Some(s) = payload.downcast_ref::<&'static str>() {
        return s.to_string();
    } else {
        "unknown payload type got".to_string()
    }
}

pub fn find_children(&self, parent: OperationId) ->Vec<OperationId>{
    self.operations
        .values
}

pub struct ExecutionStorage {
    operations: HashMap<OperationId, Operation>,
}
impl ExecutionStorage {
    pub fn new() -> Self {
        Self {
            operations: HashMap::new(),
        }
    }
    pub fn insert(&mut self, operation: Operation) {
        self.operations.insert(operation.id, operation);
    }
    pub fn get(&self, id: OperationId) -> Option<&Operation> {
        self.operations.get(&id)
    }
    pub fn get_mut(&mut self, id: OperationId) -> Option<&mut Operation> {
        self.operations.get_mut(&id)
            .values()
            .filter(|op| op.parent_id==Some(parent))
            .map(|op| op.id) // it literally extract one field ( id) from that entire operaion
            .collect()
    }
}

// thread_local! gives each thread its own private copy of the variable.
thread_local! {
    // white board for each individual thrad for storing id
    // again each thread will get tiny memory to store one id.
    // Cell is a box that lets you change what's inside, even if the box itself is not mutable.
    static CURRENT_OPERATION: Cell<Option<OperationId>> = const {
        Cell::new(None)
    };
    // creating 1 storage for all  individual thread
    //so basically  execa_stor is name of process of accessing storage but storage is its actual storage. we need name for its content we cant access it by just exec_storage
    // it actually a power we give to thread that lets access content of a struct.
    // basically we want 1 shared memory that needs to be shared among many threads. if we keep outside a thread/ operation will update it modify info
    // RefCell is a box that lets you borrow mutable access to what's inside, even if the box itself is not mutable.
    // structs with all their fields (name, status, failure_reason, etc.),
    static EXECUTION_STORAGE: RefCell<ExecutionStorage> = {
        RefCell::new(ExecutionStorage::new())
    };

}
static NEXT_OPERATION_ID: AtomicU64 = AtomicU64::new(1);

pub fn trace<F, T>(name: &str, operation_fn: F) -> T
where
    F: FnOnce() -> T,
{
    let id = OperationId(NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed));
    let parent_id = CURRENT_OPERATION.with(|current| current.get());
    let operation = Operation::new(id, name.to_string(), parent_id);

    EXECUTION_STORAGE.with(|storage| {
        storage.borrow_mut().insert(operation); // now it is like 2 buckets operation is poured into bigger bucked operations ( which has hashmap rules ). now any modifications of this opearation should be dont with accessing operations to operation with id. Technically operation dosnt exist here.  
    });

    let previous_operation = CURRENT_OPERATION.with(|current| {
        let previous = current.get();
        current.set(Some(id));
        previous
    });
    println!("[whyfail]  started: {} (id = {})", name, id.0);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation_fn()));

    let final_result = match result {
        Ok(value) => {
            EXECUTION_STORAGE.with(|storage| {
                let mut storage = storage.borrow_mut();
                if let Some(operation) = storage.get_mut(id) {
                    operation.end_time = Some(Instant::now());
                    operation.status = OperationStatus::Success;
                }
            });
            CURRENT_OPERATION.with(|current| current.set(previous_operation));
            value
        }
        Err(payload) => {
            let message = extract_message(&*payload);
            EXECUTION_STORAGE.with(|storage| {
                let mut storage = storage.borrow_mut();
                if let Some(operation) = storage.get_mut(id) {
                    operation.end_time = Some(Instant::now());
                    operation.status = OperationStatus::Failed;
                    operation.failure_reason = Some(message);
                }
            });
            CURRENT_OPERATION.with(|current| current.set(previous_operation));
            std::panic::resume_unwind(payload);
        }
    };
    final_result
}

#[cfg(test)]

mod tests {
    use super::*;

    #[test]
    //1
    fn trace_returns_operation_result() {
        let result = trace("addition", || 2 + 5);
        assert_eq!(result, 7);
    }
    #[test]
    //2
    fn trace_preserves_resul() {
        let result = trace("database_query", || Err::<(), &str>("database timeout"));
        assert_eq!(result, Err("database timeout"));
    }
    #[test]
    //3
    fn trace_return_string() {
        let result = trace(" can return str fn", || String::from("hello from whyfail"));
        assert_eq!(result, "hello from whyfail")
    }
    #[test]
    //4
    fn operation_recieve_different_ids() {
        let first = NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed);
        let second = NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed);
        println!("1st ={first}");
        println!("2nd={second}");
        assert_ne!(first, second);
    }
    #[test]
    fn operation_stores_id_and_name() {
        let id = OperationId(42);
        let start = Instant::now();

        let operation = Operation {
            id,
            name: String::from("database_query"),
            parent_id: None,
            start_time: start,
            end_time: None,
            status: OperationStatus::Running,
            failure_reason: None,
        };
        assert_eq!(operation.id.0, 42);
        assert_eq!(operation.name, "database_query");
    }
    #[test]
    //5
    fn operation_records_end_time() {
        let start = Instant::now();

        let mut operation = Operation {
            id: OperationId(1),
            name: String::from("test"),
            parent_id: None,
            start_time: start,
            end_time: None,
            status: OperationStatus::Running,
            failure_reason: None,
        };
        assert!(operation.end_time.is_none());
        operation.end_time = Some(Instant::now());
        assert!(operation.end_time.is_some());
    }
    #[test]
    //6
    fn nested_operations_restore_context() {
        assert_eq!(CURRENT_OPERATION.with(|current| current.get()), None);
        trace("outer", || {
            let outer_id = CURRENT_OPERATION.with(|current| current.get());
            assert!(outer_id.is_some());
            trace("inner", || {
                let inner_id = CURRENT_OPERATION.with(|current| current.get());
                assert!(inner_id.is_some());
                assert_ne!(inner_id, outer_id);
            });

            assert_eq!(CURRENT_OPERATION.with(|current| current.get()), outer_id);
        });
        assert_eq!(CURRENT_OPERATION.with(|current| current.get()), None);
    }
    #[test]
    //7
    fn nested_operation_has_parent() {
        trace("outer", || {
            let outer_id = CURRENT_OPERATION.with(|current| current.get().unwrap());
            trace("inner", || {
                let inner_id = CURRENT_OPERATION.with(|current| current.get().unwrap());
                assert_ne!(inner_id, outer_id)
            });
        });
    }
    #[test]
    //8
    fn operation_stores_parent_id() {
        let parent_id = OperationId(100);
        let operation_id = OperationId(20);

        let operation = Operation {
            id: operation_id,
            name: String::from("child"),
            parent_id: Some(parent_id),
            start_time: Instant::now(),
            end_time: None,
            status: OperationStatus::Running,
            failure_reason: None,
        };
        assert_eq!(operation.parent_id, Some(parent_id));
    }
    #[test]
    //9
    fn execution_store_can_store_operation() {
        let mut store = ExecutionStorage::new();
        let operation = Operation {
            id: OperationId(100),
            name: String::from("database_query"),
            parent_id: None,
            start_time: Instant::now(),
            end_time: None,
            status: OperationStatus::Running,
            failure_reason: None,
        };
        store.insert(operation);
        let stored = store.get(OperationId(100));
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().name, "database_query")
    }
    #[test]
    //10
    fn execution_storage_records_nested_operations() {
        let mut inner_id = None;
        let mut outer_id = None;
        trace("outer", || {
            outer_id = Some(CURRENT_OPERATION.with(|current| current.get().unwrap()));
            trace("inner", || {
                inner_id = Some(CURRENT_OPERATION.with(|current| current.get().unwrap()));
            })
        });
        EXECUTION_STORAGE.with(|storage| {
            let storage_ref = storage.borrow();
            let a = storage_ref.get(outer_id.unwrap());
            let b = storage_ref.get(inner_id.unwrap());
            assert!(a.is_some());
            assert!(b.is_some());

            let outer_op = a.unwrap();
            let inner_op = b.unwrap();
            assert!(inner_op.parent_id == Some(outer_id.unwrap()));
            assert!(outer_op.end_time.is_some());
            assert!(inner_op.end_time.is_some());
        })
    }
    #[test]
    //11
    fn operation_starts_as_running() {
        let id = OperationId(1);
        let op = Operation::new(id, "test".to_string(), None);
        assert_eq!(op.status, OperationStatus::Running);
    }

    #[test]
    //12
    fn panicking_operation_is_detected() {
        let result = std::panic::catch_unwind(|| {
            trace("failing_operation", || {
                panic!("something wnent wrong");
            });
        });
        assert!(result.is_err());
    }
    #[test]
    //13
    fn panicking_operation_records_str_message() {
        let mut op_id = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            trace("str_panic", || {
                op_id = CURRENT_OPERATION.with(|current| current.get());
                panic!("this is a &str panic message");
            });
        }));
        assert!(result.is_err());
        let id = op_id.unwrap();
        EXECUTION_STORAGE.with(|storage| {
            let storage = storage.borrow();
            let op = storage.get(id).unwrap();
            assert_eq!(
                op.failure_reason.as_deref(),
                Some("this is a &str panic message")
            )
        })
    }
    #[test]
    //14
    fn panicking_operation_records_string_message() {
        let mut op_id = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            trace("string_panic", || {
                op_id = CURRENT_OPERATION.with(|current| current.get());
                panic!("this is a string panic message");
            });
        }));
        assert!(result.is_err());
        let id = op_id.unwrap();
        EXECUTION_STORAGE.with(|storage| {
            let storage = storage.borrow();
            let op = storage.get(id).unwrap();
            assert_eq!(
                op.failure_reason.as_deref(),
                Some("this is a string panic message")
            );
        })
    }
}
