use ssz_types::{
    typenum::{U1048576, U1073741824},
    VariableList,
};

pub type MaxBytesPerTransaction = U1073741824; // 1,073,741,824
pub type MaxTransactionsPerPayload = U1048576; // 1,048,576

pub type SszTransaction = VariableList<u8, MaxBytesPerTransaction>;
pub type SszTransactions = VariableList<SszTransaction, MaxTransactionsPerPayload>;
